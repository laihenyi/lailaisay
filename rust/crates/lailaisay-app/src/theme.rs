//! Locked lailaisay dark tokens (OKLch → Color32). Do not invent a second accent.
//!
//! Round 3 adds opaque Liquid Glass chrome fills. egui has no backdrop blur:
//! approximate with solid fills + 1px top highlight + 1px edge + soft shadow.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Frame, Margin, Painter, Rect,
    Shadow, Stroke, Visuals,
};

static FONTS_INSTALLED: AtomicBool = AtomicBool::new(false);

pub const BG: Color32 = Color32::from_rgb(12, 16, 21);
pub const SURFACE: Color32 = Color32::from_rgb(22, 27, 33);
pub const FG: Color32 = Color32::from_rgb(233, 235, 238);
pub const MUTED: Color32 = Color32::from_rgb(167, 171, 176);
pub const BORDER: Color32 = Color32::from_rgb(48, 52, 58);
pub const ACCENT: Color32 = Color32::from_rgb(18, 118, 206);
pub const SUCCESS: Color32 = Color32::from_rgb(20, 135, 78);
pub const WARN: Color32 = Color32::from_rgb(187, 136, 26);
pub const DANGER: Color32 = Color32::from_rgb(207, 66, 56);

/// Const stand-in for `Color32::from_rgba_unmultiplied` (not const in egui 0.32).
const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_premultiplied(
        ((r as u16 * a as u16) / 255) as u8,
        ((g as u16 * a as u16) / 255) as u8,
        ((b as u16 * a as u16) / 255) as u8,
        a,
    )
}

/// Shell chrome: title, statusbar, rail, footer (opaque glass-fill stand-in).
pub const GLASS_FILL: Color32 = Color32::from_rgb(18, 23, 28);
/// Chip, rail selected, secondary buttons.
pub const GLASS_FILL_DEEP: Color32 = Color32::from_rgb(28, 34, 41);
/// 1px top inner highlight on glass chrome.
pub const GLASS_HIGHLIGHT: Color32 = rgba(255, 255, 255, 90);
/// 1px outer edge on glass chrome.
pub const GLASS_EDGE: Color32 = rgba(255, 255, 255, 46);
/// Hover overlay only — no translate / bounce.
pub const GLASS_HOVER: Color32 = rgba(255, 255, 255, 26);
/// Tinted glass fill for the single solid button（儲存設定）.
pub const GLASS_TINT: Color32 = Color32::from_rgb(20, 110, 190);
pub const GLASS_TINT_HOVER: Color32 = Color32::from_rgb(34, 124, 204);
pub const GLASS_TINT_HIGHLIGHT: Color32 = rgba(255, 255, 255, 97);

pub const RAIL_W: f32 = 156.0;
pub const ROW_H: f32 = 36.0;
pub const ROUND_CTL: f32 = 12.0;
/// Window shell inner Frame. If the OS window cannot round, only this Frame is 26.
pub const ROUND_WIN: f32 = 26.0;
/// Solid content pane (never glass-on-glass).
pub const ROUND_PANE: f32 = 10.0;
pub const ROUND_KEY: f32 = 8.0;

pub const PANEL_SHADOW: Shadow = Shadow {
    offset: [0, 18],
    blur: 60,
    spread: 0,
    color: rgba(0, 0, 0, 140),
};

pub const CONTROL_SHADOW: Shadow = Shadow {
    offset: [0, 2],
    blur: 8,
    spread: 0,
    color: rgba(0, 0, 0, 90),
};

pub fn apply_visuals(ctx: &egui::Context) {
    if !FONTS_INSTALLED.swap(true, Ordering::Relaxed) {
        install_cjk_fonts(ctx);
    }
    ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::Dark);
    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(FG);
    // Chrome panels default to glass-fill; the content pane Frame sets SURFACE.
    visuals.panel_fill = GLASS_FILL;
    visuals.window_fill = BG;
    visuals.extreme_bg_color = BG;
    visuals.window_corner_radius = CornerRadius::same(ROUND_WIN as u8);
    visuals.window_stroke = Stroke::new(1.0_f32, GLASS_EDGE);
    visuals.window_shadow = PANEL_SHADOW;
    visuals.widgets.noninteractive.fg_stroke.color = FG;
    visuals.widgets.inactive.fg_stroke.color = FG;
    visuals.widgets.hovered.fg_stroke.color = FG;
    visuals.widgets.active.fg_stroke.color = FG;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(32, 38, 46);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(36, 44, 54);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(ROUND_CTL as u8);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(ROUND_CTL as u8);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(ROUND_CTL as u8);
    visuals.widgets.active.corner_radius = CornerRadius::same(ROUND_CTL as u8);
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.hyperlink_color = ACCENT;
    visuals.widgets.open.fg_stroke.color = FG;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.interact_size.y = 28.0;
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    ctx.set_style(style);
}

/// Shell / clear_color is BG, not the content SURFACE.
pub fn clear_color() -> [f32; 4] {
    [
        BG.r() as f32 / 255.0,
        BG.g() as f32 / 255.0,
        BG.b() as f32 / 255.0,
        1.0,
    ]
}

pub fn muted_fill(alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(MUTED.r(), MUTED.g(), MUTED.b(), (alpha * 255.0) as u8)
}

pub fn fg_fill(alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(FG.r(), FG.g(), FG.b(), (alpha * 255.0) as u8)
}

pub fn glass_edge_stroke() -> Stroke {
    Stroke::new(1.0_f32, GLASS_EDGE)
}

/// Capsule rounding: `height / 2`.
pub fn pill(height: f32) -> CornerRadius {
    CornerRadius::same((height * 0.5).round().clamp(0.0, 255.0) as u8)
}

/// Concentric rule: inner radius = outer − padding (floored at 0).
pub fn inner_radius(outer: f32, padding: f32) -> f32 {
    (outer - padding).max(0.0)
}

/// Flush chrome panels only: 1px strip on the inner top edge.
/// Do not use this on pills — a full-width `hline` chords through the caps.
pub fn paint_top_highlight(painter: &Painter, rect: Rect, color: Color32) {
    if rect.width() < 2.0 || rect.height() < 1.0 {
        return;
    }
    let hl = Rect::from_min_max(
        egui::pos2(rect.left() + 1.0, rect.top()),
        egui::pos2(rect.right() - 1.0, rect.top() + 1.0),
    );
    painter.rect_filled(hl, 0.0, color);
}

/// Inset 1px highlight for rounded glass controls (chip, rail, keycap, buttons).
///
/// Span is `width − 2×radius` at `rect.top() + 1`. A full-width strip here
/// would chord through the pill — do not use [`paint_top_highlight`] on these.
pub fn control_top_highlight_rect(rect: Rect, rounding: CornerRadius) -> Option<Rect> {
    let radius = u8::max(rounding.nw, rounding.ne) as f32;
    let left = rect.left() + radius;
    let right = rect.right() - radius;
    let top = rect.top() + 1.0;
    if right - left < 1.0 || rect.height() < 3.0 {
        return None;
    }
    Some(Rect::from_min_max(
        egui::pos2(left, top),
        egui::pos2(right, top + 1.0),
    ))
}

pub fn paint_control_top_highlight(painter: &Painter, rect: Rect, rounding: CornerRadius) {
    if let Some(hl) = control_top_highlight_rect(rect, rounding) {
        painter.rect_filled(hl, 0.0, GLASS_HIGHLIGHT);
    }
}

pub fn shrink_rounding(rounding: CornerRadius, by: f32) -> CornerRadius {
    let shrink = |v: u8| -> u8 { (v as f32 - by).max(0.0) as u8 };
    CornerRadius {
        nw: shrink(rounding.nw),
        ne: shrink(rounding.ne),
        sw: shrink(rounding.sw),
        se: shrink(rounding.se),
    }
}

/// Pill / rounded chrome: fill + optional 1px concentric edge + optional shadow.
///
/// When `top_highlight` is set, paint a radius-inset 1px strip at `top + 1`
/// (not a full-width chord through the middle of the control).
pub fn paint_glass_control(
    painter: &Painter,
    rect: Rect,
    rounding: CornerRadius,
    fill: Color32,
    edge: Option<Color32>,
    shadow: bool,
    top_highlight: bool,
) {
    if shadow {
        paint_control_shadow(painter, rect, rounding);
    }
    if let Some(edge) = edge {
        painter.rect_filled(rect, rounding, edge);
        let inner = rect.shrink(1.0);
        painter.rect_filled(inner, shrink_rounding(rounding, 1.0), fill);
    } else {
        painter.rect_filled(rect, rounding, fill);
    }
    if top_highlight {
        paint_control_top_highlight(painter, rect, rounding);
    }
}

pub fn paint_control_shadow(painter: &Painter, rect: Rect, rounding: CornerRadius) {
    painter.add(CONTROL_SHADOW.as_shape(rect, rounding));
}

/// Opaque rounded shell. Does not make the OS window transparent.
pub fn paint_shell_frame(ctx: &egui::Context) {
    let rect = ctx.screen_rect();
    let rounding = CornerRadius::same(ROUND_WIN as u8);
    let painter = ctx.layer_painter(egui::LayerId::background());
    painter.add(PANEL_SHADOW.as_shape(rect, rounding));
    painter.rect(
        rect,
        rounding,
        BG,
        glass_edge_stroke(),
        egui::StrokeKind::Inside,
    );
    paint_top_highlight(&painter, rect, GLASS_HIGHLIGHT);
}

/// Status / rail / footer: glass-fill, 1px edge, flush (no rounding).
pub fn chrome_panel_frame() -> Frame {
    Frame::new()
        .fill(GLASS_FILL)
        .stroke(glass_edge_stroke())
        .corner_radius(CornerRadius::ZERO)
}

/// Solid content pane sitting on the shell — never glass-on-glass.
/// Extra top inset keeps the first CJK heading below the rounded clip and
/// the status-strip seam (18px all around used to sit glyphs on the radius).
pub fn content_pane_frame() -> Frame {
    Frame::new()
        .fill(SURFACE)
        .corner_radius(CornerRadius::same(ROUND_PANE as u8))
        .inner_margin(Margin {
            left: 18,
            right: 18,
            top: 22,
            bottom: 18,
        })
        .outer_margin(Margin {
            left: 0,
            right: 8,
            top: 10,
            bottom: 0,
        })
}

/// macOS first (PingFang / Heiti), then optional Linux Noto. `TOK_CJK_FONT` wins.
pub fn cjk_font_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("TOK_CJK_FONT") {
        if !p.is_empty() {
            out.push(PathBuf::from(p));
        }
    }
    const MAC: &[&str] = &[
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Supplemental/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/Hiragino Sans GB W3.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    ];
    const LINUX: &[&str] = &[
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJKtc-Regular.otf",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansTC-Regular.otf",
        "/usr/share/fonts/truetype/arphic/uming.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    ];
    if cfg!(target_os = "macos") {
        out.extend(MAC.iter().map(PathBuf::from));
    } else if cfg!(target_os = "windows") {
        let root =
            PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| r"C:\Windows".into()));
        out.extend(
            ["msjh.ttc", "msyh.ttc", "mingliu.ttc", "simsun.ttc"]
                .iter()
                .map(|name| root.join("Fonts").join(name)),
        );
    } else {
        out.extend(LINUX.iter().map(PathBuf::from));
    }
    out
}

/// Faces that cover Misc Technical (`⌥ ⇧ ⌘ ⌃`). Tried in order; first readable file wins.
pub fn symbol_font_candidates() -> Vec<PathBuf> {
    const MAC: &[&str] = &[
        "/System/Library/Fonts/SFNS.ttf",
        "/System/Library/Fonts/SFNSText.ttf",
        "/System/Library/Fonts/SFCompact.ttf",
        "/System/Library/Fonts/SFNSRounded.ttf",
        "/Library/Fonts/SF-Pro.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    ];
    const LINUX: &[&str] = &[
        "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ];
    MAC.iter().chain(LINUX.iter()).map(PathBuf::from).collect()
}

fn ttc_num_fonts(bytes: &[u8]) -> u32 {
    // TTC: 'ttcf' + version (4) + numFonts (4)
    if bytes.len() >= 12 && bytes.starts_with(b"ttcf") {
        u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]).clamp(1, 32)
    } else {
        1
    }
}

/// First parseable face in `bytes` (TTC index 0…n). None if ab_glyph rejects the file.
pub fn font_data_from_cjk_bytes(bytes: Vec<u8>) -> Option<FontData> {
    let n = ttc_num_fonts(&bytes).max(1);
    for index in 0..n {
        if ab_glyph::FontRef::try_from_slice_and_index(&bytes, index).is_ok() {
            let mut data = FontData::from_owned(bytes);
            data.index = index;
            return Some(data);
        }
    }
    None
}

pub fn load_first_cjk_font(paths: &[PathBuf]) -> Option<(PathBuf, FontData)> {
    for path in paths {
        if !path.is_file() {
            continue;
        }
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        if let Some(data) = font_data_from_cjk_bytes(bytes) {
            return Some((path.clone(), data));
        }
    }
    None
}

const BUNDLED_CJK_FONT: &[u8] = include_bytes!("../assets/fonts/NotoSansCJKtc-Regular.otf");

fn install_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    // Always retain a CJK fallback, including on Windows without language packs.
    fonts.font_data.insert(
        "lailaisay_cjk_fallback".into(),
        Arc::new(FontData::from_static(BUNDLED_CJK_FONT)),
    );
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("lailaisay_cjk_fallback".into());
    }

    // Symbols before CJK on Proportional so PingFang's missing ⌥ does not paint tofu.
    // Monospace keeps built-in mono first so the `fn` keycap stays a mono face.
    if let Some((path, data)) = load_first_cjk_font(&symbol_font_candidates()) {
        let index = data.index;
        fonts
            .font_data
            .insert("lailaisay_symbols".to_owned(), Arc::new(data));
        if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
            prop.insert(0, "lailaisay_symbols".to_owned());
        }
        if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
            if !mono.iter().any(|n| n == "lailaisay_symbols") {
                mono.push("lailaisay_symbols".to_owned());
            }
        }
        eprintln!(
            "[lailaisay-app] symbol font {} (face {index})",
            path.display()
        );
    }

    if let Some((path, data)) = load_first_cjk_font(&cjk_font_candidates()) {
        let index = data.index;
        fonts
            .font_data
            .insert("lailaisay_cjk".to_owned(), Arc::new(data));
        if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
            let idx = usize::from(prop.first().is_some_and(|n| n == "lailaisay_symbols"));
            prop.insert(idx, "lailaisay_cjk".to_owned());
        }
        if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
            if !mono.iter().any(|n| n == "lailaisay_cjk") {
                mono.push("lailaisay_cjk".to_owned());
            }
        }
        eprintln!("[lailaisay-app] CJK font {} (face {index})", path.display());
    } else {
        eprintln!("[lailaisay-app] using bundled Noto Sans TC fallback");
    }

    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_candidates_include_macos_pingfang() {
        let mac = [
            PathBuf::from("/System/Library/Fonts/PingFang.ttc"),
            PathBuf::from("/System/Library/Fonts/STHeiti Light.ttc"),
        ];
        if cfg!(target_os = "macos") {
            let got = cjk_font_candidates();
            for p in mac {
                assert!(got.contains(&p), "missing {}", p.display());
            }
        } else {
            assert!(!cjk_font_candidates().is_empty());
        }
    }

    #[test]
    fn bundled_font_has_chinese_glyphs_and_outlines() {
        use ab_glyph::Font;
        let font = ab_glyph::FontRef::try_from_slice(BUNDLED_CJK_FONT).unwrap();
        for c in "設定一般語音模型潤稿自訂辭典權限儲存麥克風錄製辨識臺灣简体中文".chars()
        {
            let id = font.glyph_id(c);
            assert_ne!(id.0, 0, "missing glyph: {c}");
            assert!(
                font.outline_glyph(id.with_scale(18.0)).is_some(),
                "missing outline: {c}"
            );
        }
    }

    #[test]
    fn missing_cjk_file_is_skipped() {
        assert!(load_first_cjk_font(&[PathBuf::from("/no/such/lailaisay-cjk.ttf")]).is_none());
    }

    #[test]
    fn symbol_candidates_cover_sf_pro_and_noto_symbols() {
        let got = symbol_font_candidates();
        assert!(
            got.iter().any(|p| {
                let s = p.to_string_lossy();
                s.contains("SFNS") || s.contains("SF-Pro")
            }),
            "missing SF Pro / SFNS path"
        );
        assert!(
            got.iter()
                .any(|p| p.to_string_lossy().contains("NotoSansSymbols")),
            "missing Noto Sans Symbols fallback"
        );
    }

    #[test]
    fn garbage_bytes_are_not_installed() {
        assert!(font_data_from_cjk_bytes(b"not-a-font".to_vec()).is_none());
    }

    #[test]
    fn r1_status_and_type_tokens_unchanged() {
        assert_eq!(FG, Color32::from_rgb(233, 235, 238));
        assert_eq!(MUTED, Color32::from_rgb(167, 171, 176));
        assert_eq!(ACCENT, Color32::from_rgb(18, 118, 206));
        assert_eq!(SUCCESS, Color32::from_rgb(20, 135, 78));
        assert_eq!(WARN, Color32::from_rgb(187, 136, 26));
        assert_eq!(DANGER, Color32::from_rgb(207, 66, 56));
        assert_eq!(SURFACE, Color32::from_rgb(22, 27, 33));
    }

    #[test]
    fn dark_glass_tokens_match_r3_table() {
        assert_eq!(BG, Color32::from_rgb(12, 16, 21));
        assert_eq!(GLASS_FILL, Color32::from_rgb(18, 23, 28));
        assert_eq!(GLASS_FILL_DEEP, Color32::from_rgb(28, 34, 41));
        assert_eq!(SURFACE, Color32::from_rgb(22, 27, 33));
        assert_eq!(
            GLASS_HIGHLIGHT,
            Color32::from_rgba_unmultiplied(255, 255, 255, 90)
        );
        assert_eq!(
            GLASS_EDGE,
            Color32::from_rgba_unmultiplied(255, 255, 255, 46)
        );
        assert_eq!(
            GLASS_HOVER,
            Color32::from_rgba_unmultiplied(255, 255, 255, 26)
        );
        assert_eq!(GLASS_TINT, Color32::from_rgb(20, 110, 190));
        assert_eq!(GLASS_TINT_HOVER, Color32::from_rgb(34, 124, 204));
        assert_eq!(
            GLASS_TINT_HIGHLIGHT,
            Color32::from_rgba_unmultiplied(255, 255, 255, 97)
        );
        assert_eq!(PANEL_SHADOW.offset, [0, 18]);
        assert_eq!(PANEL_SHADOW.blur, 60);
        assert_eq!(PANEL_SHADOW.spread, 0);
        assert_eq!(
            PANEL_SHADOW.color,
            Color32::from_rgba_unmultiplied(0, 0, 0, 140)
        );
        assert_eq!(CONTROL_SHADOW.offset, [0, 2]);
        assert_eq!(CONTROL_SHADOW.blur, 8);
        assert_eq!(CONTROL_SHADOW.spread, 0);
        assert_eq!(
            CONTROL_SHADOW.color,
            Color32::from_rgba_unmultiplied(0, 0, 0, 90)
        );
    }

    #[test]
    fn concentric_radii_and_pills() {
        assert_eq!(ROUND_WIN, 26.0);
        assert_eq!(ROUND_PANE, 10.0);
        assert_eq!(ROUND_KEY, 8.0);
        assert_eq!(inner_radius(ROUND_WIN, 16.0), ROUND_PANE);
        assert_eq!(pill(ROW_H), CornerRadius::same(18));
        assert_eq!(pill(28.0), CornerRadius::same(14));
    }

    #[test]
    fn clear_color_is_shell_bg_not_surface() {
        let c = clear_color();
        assert!((c[0] - BG.r() as f32 / 255.0).abs() < 1e-5);
        assert!((c[1] - BG.g() as f32 / 255.0).abs() < 1e-5);
        assert!((c[2] - BG.b() as f32 / 255.0).abs() < 1e-5);
        assert_eq!(c[3], 1.0);
        assert_ne!(BG, SURFACE);
    }

    #[test]
    fn apply_visuals_forces_dark_and_splits_panel_fills() {
        let ctx = egui::Context::default();
        apply_visuals(&ctx);
        assert_eq!(
            ctx.options(|o| o.theme_preference),
            egui::ThemePreference::Dark
        );
        let style = ctx.style();
        assert!(style.visuals.dark_mode);
        assert_eq!(style.visuals.panel_fill, GLASS_FILL);
        assert_eq!(style.visuals.window_fill, BG);
        assert_eq!(style.visuals.extreme_bg_color, BG);
        assert_eq!(
            style.visuals.window_corner_radius,
            CornerRadius::same(ROUND_WIN as u8)
        );
    }

    #[test]
    fn chrome_and_content_frames_use_distinct_fills() {
        let chrome = chrome_panel_frame();
        let pane = content_pane_frame();
        assert_eq!(chrome.fill, GLASS_FILL);
        assert_eq!(chrome.stroke.color, GLASS_EDGE);
        assert_eq!(chrome.corner_radius, CornerRadius::ZERO);
        assert_eq!(pane.fill, SURFACE);
        assert_eq!(pane.stroke, Stroke::NONE);
        assert_eq!(pane.corner_radius, CornerRadius::same(ROUND_PANE as u8));
        assert_ne!(chrome.fill, pane.fill);
        assert_eq!(pane.inner_margin.top, 22);
        assert_eq!(pane.outer_margin.top, 10);
    }

    #[test]
    fn control_top_highlight_is_inset_strip_not_full_width() {
        let rect = Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(100.0, 28.0));
        let rounding = pill(28.0);
        let hl = control_top_highlight_rect(rect, rounding).expect("highlight");
        assert!((hl.height() - 1.0).abs() < 0.01);
        assert!((hl.top() - (rect.top() + 1.0)).abs() < 0.01);
        let radius = rounding.nw as f32;
        assert!((hl.width() - (rect.width() - 2.0 * radius)).abs() < 0.01);
        assert!((hl.left() - (rect.left() + radius)).abs() < 0.01);
        assert!((hl.right() - (rect.right() - radius)).abs() < 0.01);
        assert!(hl.left() > rect.left() + 1.0);
        assert!(hl.right() < rect.right() - 1.0);
        assert_ne!(
            hl,
            Rect::from_min_max(
                egui::pos2(rect.left(), rect.center().y),
                egui::pos2(rect.right(), rect.center().y + 1.0),
            ),
            "must not be a mid-control full-width bar"
        );
        assert_eq!(
            GLASS_HIGHLIGHT,
            Color32::from_rgba_unmultiplied(255, 255, 255, 90)
        );
    }

    #[test]
    fn titlebar_status_divider_is_glass_edge_not_gray() {
        assert_eq!(
            GLASS_EDGE,
            Color32::from_rgba_unmultiplied(255, 255, 255, 46)
        );
        assert_ne!(GLASS_EDGE, Color32::from_rgb(186, 187, 188));
        assert_eq!(chrome_panel_frame().stroke.color, GLASS_EDGE);
        assert_eq!(GLASS_FILL, Color32::from_rgb(18, 23, 28));
    }
}
