//! Dock-icon policy shared by the macOS host and Linux CI.
//!
//! Launch Services treats `LSUIElement` apps as agents. Switching to
//! `NSApplicationActivationPolicyRegular` at runtime can still leave Dock with
//! a bundle-id monogram (the bundle identifier) unless the process also
//! assigns `NSApplication.applicationIconImage` from `AppIcon.icns`.
//!
//! `CFBundleIconName` without `Assets.car` is a second, independent way Dock
//! ignores the packaged icns. Packaging must keep `CFBundleIconFile` only.

/// `NSApplicationActivationPolicyRegular` — ordinary app, Dock tile visible.
pub const ACTIVATION_POLICY_REGULAR: usize = 0;
/// `NSApplicationActivationPolicyAccessory` — menu-bar / tray only.
pub const ACTIVATION_POLICY_ACCESSORY: usize = 1;

/// Activation policy that matches the Settings 「顯示 Dock 圖示」 switch.
pub fn activation_policy_code(show_dock_icon: bool) -> usize {
    if show_dock_icon {
        ACTIVATION_POLICY_REGULAR
    } else {
        ACTIVATION_POLICY_ACCESSORY
    }
}

/// Regular policy must also push the waveform icns onto `applicationIconImage`.
pub fn should_apply_bundled_app_icon(show_dock_icon: bool) -> bool {
    show_dock_icon
}

/// Bundle path, then `NSImage.imageNamed("AppIcon")`, then embedded icns bytes.
///
/// Callers must pass closures. `Option::or_else` does not accept an `unsafe fn`
/// item (E0277 on macOS after the Dock AppIcon loaders landed).
pub fn first_present<T>(
    primary: Option<T>,
    named: impl FnOnce() -> Option<T>,
    embedded: impl FnOnce() -> Option<T>,
) -> Option<T> {
    primary.or_else(named).or_else(embedded)
}

/// PNG-in-icns types Dock / Finder actually use on modern macOS.
pub const REQUIRED_ICNS_KINDS: &[&[u8; 4]] = &[
    b"icp4", // 16
    b"icp5", // 32
    b"icp6", // 64
    b"ic07", // 128
    b"ic08", // 256
    b"ic09", // 512
    b"ic10", // 1024 — retina Dock
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcnsChunk {
    pub kind: [u8; 4],
    pub payload: Vec<u8>,
}

/// Parse a PNG-in-icns container. Rejects truncated or non-icns bytes.
pub fn parse_icns(data: &[u8]) -> Result<Vec<IcnsChunk>, String> {
    if data.len() < 8 || &data[..4] != b"icns" {
        return Err("not an icns file".into());
    }
    let declared = u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize;
    if declared != data.len() {
        return Err(format!(
            "icns size {} does not match bytes {}",
            declared,
            data.len()
        ));
    }
    let mut chunks = Vec::new();
    let mut off = 8usize;
    while off + 8 <= data.len() {
        let kind: [u8; 4] = data[off..off + 4].try_into().unwrap();
        let size = u32::from_be_bytes(data[off + 4..off + 8].try_into().unwrap()) as usize;
        if size < 8 || off + size > data.len() {
            return Err(format!("icns chunk {:?} has invalid size {size}", kind));
        }
        chunks.push(IcnsChunk {
            kind,
            payload: data[off + 8..off + size].to_vec(),
        });
        off += size;
    }
    if off != data.len() {
        return Err("icns trailing bytes after last chunk".into());
    }
    Ok(chunks)
}

pub fn icns_has_required_dock_sizes(chunks: &[IcnsChunk]) -> bool {
    REQUIRED_ICNS_KINDS.iter().all(|kind| {
        chunks
            .iter()
            .any(|c| &c.kind == *kind && c.payload.starts_with(b"\x89PNG"))
    })
}

/// Info.plist must advertise the icns file, stay an agent by default, and must
/// not name an asset-catalog icon unless `Assets.car` is also shipped.
pub fn info_plist_dock_icon_ok(plist: &str, assets_car_present: bool) -> Result<(), String> {
    if !plist.contains("<key>CFBundleIconFile</key>") || !plist.contains("<string>AppIcon</string>")
    {
        return Err("Info.plist must set CFBundleIconFile to AppIcon".into());
    }
    if !plist.contains("<key>LSUIElement</key>") {
        return Err("Info.plist must set LSUIElement for menu-bar accessory launch".into());
    }
    if !plist.contains("<string>com.yikai.lailaisay</string>") {
        return Err("Info.plist must keep bundle id com.yikai.lailaisay".into());
    }
    if plist.contains("xyz.2qs.Tok") {
        return Err("Info.plist must not use xyz.2qs.Tok".into());
    }
    if plist.contains("<key>CFBundleIconName</key>") && !assets_car_present {
        return Err(
            "CFBundleIconName without Assets.car makes Dock ignore AppIcon.icns (monogram fallback)"
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn macos_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../macos")
    }

    #[test]
    fn regular_policy_applies_bundled_icon_accessory_does_not() {
        assert_eq!(activation_policy_code(true), ACTIVATION_POLICY_REGULAR);
        assert_eq!(activation_policy_code(false), ACTIVATION_POLICY_ACCESSORY);
        assert!(should_apply_bundled_app_icon(true));
        assert!(!should_apply_bundled_app_icon(false));
    }

    #[test]
    fn first_present_wraps_fn_items_in_closures() {
        fn named() -> Option<&'static str> {
            Some("NSImage.imageNamed")
        }
        unsafe fn embedded() -> Option<&'static str> {
            Some("embedded.icns")
        }
        assert_eq!(
            first_present(Some("bundle.icns"), || named(), || unsafe { embedded() }),
            Some("bundle.icns")
        );
        assert_eq!(
            first_present(None, || named(), || unsafe { embedded() }),
            Some("NSImage.imageNamed")
        );
        assert_eq!(
            first_present(None, || None, || unsafe { embedded() }),
            Some("embedded.icns")
        );
        assert_eq!(first_present(None::<&str>, || None, || None), None);
    }

    #[test]
    fn packaged_info_plist_uses_icns_not_missing_asset_catalog() {
        let plist = std::fs::read_to_string(macos_dir().join("Info.plist")).unwrap();
        let assets = macos_dir().join("Assets.car").is_file();
        info_plist_dock_icon_ok(&plist, assets).unwrap();
    }

    #[test]
    fn packaged_appicon_icns_has_dock_png_sizes() {
        let bytes = std::fs::read(macos_dir().join("AppIcon.icns")).unwrap();
        let chunks = parse_icns(&bytes).expect("AppIcon.icns");
        assert!(
            icns_has_required_dock_sizes(&chunks),
            "AppIcon.icns missing Dock PNG sizes; have {:?}",
            chunks
                .iter()
                .map(|c| String::from_utf8_lossy(&c.kind).into_owned())
                .collect::<Vec<_>>()
        );
        assert!(
            chunks
                .iter()
                .any(|c| c.kind == *b"ic10" && c.payload.len() > 1024),
            "1024px ic10 must be a real bitmap, not a stub"
        );
    }

    #[test]
    fn parse_icns_rejects_garbage() {
        assert!(parse_icns(b"nope").is_err());
        assert!(info_plist_dock_icon_ok("<plist/>", false).is_err());
        assert!(info_plist_dock_icon_ok(
            "<key>CFBundleIconFile</key><string>AppIcon</string>\
             <key>LSUIElement</key><true/>\
             <string>com.yikai.lailaisay</string>\
             <key>CFBundleIconName</key><string>AppIcon</string>",
            false
        )
        .is_err());
    }
}
