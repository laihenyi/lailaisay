//! Choose how to paste for the active application.

use std::fmt;

/// App that should receive the paste (captured at hotkey-down, not at paste time).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PasteTarget {
    pub bundle_id: String,
    pub app_name: Option<String>,
    pub ax_role: Option<String>,
    pub ax_value_len: Option<usize>,
    /// Windows `HWND` captured at hotkey-down (`isize` so the type is portable).
    pub hwnd: Option<isize>,
}

impl PasteTarget {
    pub fn is_empty(&self) -> bool {
        self.bundle_id.is_empty() && self.app_name.is_none() && self.hwnd.is_none()
    }
}

impl fmt::Display for PasteTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({})",
            if self.bundle_id.is_empty() {
                "?"
            } else {
                self.bundle_id.as_str()
            },
            self.app_name.as_deref().unwrap_or("?")
        )?;
        if let Some(role) = &self.ax_role {
            write!(f, " ax={role}")?;
        }
        if let Some(n) = self.ax_value_len {
            write!(f, " value_len={n}")?;
        }
        if let Some(hwnd) = self.hwnd {
            write!(f, " hwnd={hwnd:#x}")?;
        }
        Ok(())
    }
}

/// True when `haystack` contains `text` or a stable prefix of it.
fn ax_haystack_has_needle(haystack: Option<&str>, text: &str) -> bool {
    let needle = text.trim();
    if needle.is_empty() {
        return false;
    }
    let Some(haystack) = haystack.filter(|s| !s.is_empty()) else {
        return false;
    };
    if haystack.contains(needle) {
        return true;
    }
    let prefix: String = needle.chars().take(24).collect();
    prefix.chars().count() >= 4 && haystack.contains(&prefix)
}

/// True when the focused field's AX value/selection now contains `text`
/// and differs from the pre-paste snapshot. Used so CGEvent ⌘V cannot
/// report success after a no-op `CGEventPost`.
pub fn ax_value_shows_paste(before: Option<&str>, after: Option<&str>, text: &str) -> bool {
    if !ax_haystack_has_needle(after, text) {
        return false;
    }
    match (before, after) {
        (Some(b), Some(a)) if b == a => false,
        _ => true,
    }
}

/// Terminal / terminal-emulator bundles whose AX value is often the
/// scrollback or a stale prompt, not the input line. A successful
/// Edit → Paste / ⌘V script must not be followed by another clipboard paste
/// just because this snapshot did not grow.
pub fn is_ax_flaky_terminal_bundle(bundle_id: &str) -> bool {
    let b = bundle_id.to_ascii_lowercase();
    b.contains("com.apple.terminal")
        || b.contains("com.googlecode.iterm2")
        || b.contains("dev.warp.warp")
        || b.contains("com.mitchellh.ghostty")
        || b.contains("org.alacritty")
        || b.contains("io.alacritty")
        || b.contains("net.kovidgoyal.kitty")
        // Cursor (ToDesktop id) embeds a terminal; AX is the same class of quirk.
        || b.contains("todesktop.230313mzl4w4u92")
        || b.contains("com.cursor.")
        || b == "com.cursor"
}

/// After MenuPaste / AppleScript ⌘V reported click / keystroke success,
/// should we run another clipboard-inserting strategy (second Cmd+V)?
///
/// Default is **no**: the first script likely already inserted. Only
/// return `true` when AX is considered reliable and the focused field
/// is still missing `text`. Terminals, missing AX, an unchanged field
/// that already contains the needle, and any inconclusive snapshot
/// must not attempt a second clipboard paste.
pub fn should_attempt_second_clipboard_paste(
    bundle_id: &str,
    before: Option<&str>,
    after: Option<&str>,
    text: &str,
) -> bool {
    if ax_value_shows_paste(before, after, text) {
        return false;
    }
    // Already in the pre-paste snapshot (or AX captured it late). Another
    // Cmd+V would duplicate the sentence.
    if ax_haystack_has_needle(before, text) {
        return false;
    }
    if before.is_none() && after.is_none() {
        return false;
    }
    if is_ax_flaky_terminal_bundle(bundle_id) {
        return false;
    }
    // Reliable AX: retry only when the live field value still lacks `text`.
    !ax_haystack_has_needle(after, text)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteStrategy {
    /// Native apps: System Events "Edit → Paste".
    MenuPaste,
    /// Electron / Chromium (incl. VS Code): AppleScript ⌘V.
    AppleScriptCmdV,
    /// CGEvent ⌘V — posted events are **not** success unless AX verifies.
    CgEventCmdV,
    /// `useClipboardPaste == false`: type the characters.
    TypeText,
    /// AX `kAXSelectedTextAttribute` insert at the focused element.
    AxInsert,
}

/// Bundle-id heuristics for the platform paste client.
pub fn is_electron_or_chromium(bundle_id: &str) -> bool {
    bundle_id.contains("microsoft.VSCode")
        || bundle_id.contains("Electron")
        || bundle_id.contains("com.google.Chrome")
        || bundle_id.contains("com.brave.Browser")
        || bundle_id.contains("com.microsoft.edgemac")
        || bundle_id.contains("Slack")
        || bundle_id.contains("Discord")
}

pub fn is_vscode(bundle_id: &str) -> bool {
    bundle_id.contains("microsoft.VSCode")
}

/// Primary strategy for a frontmost app. AX insert is a last-resort fallback,
/// not the first attempt (many apps do not support `kAXSelectedText`).
pub fn choose_strategy(bundle_id: &str, use_clipboard_paste: bool) -> PasteStrategy {
    if !use_clipboard_paste {
        return PasteStrategy::TypeText;
    }
    if is_electron_or_chromium(bundle_id) {
        PasteStrategy::AppleScriptCmdV
    } else {
        PasteStrategy::MenuPaste
    }
}

pub fn fallback_after(primary: PasteStrategy) -> &'static [PasteStrategy] {
    match primary {
        // AppleScript + AX before CGEvent. CGEventPost is not proof of paste.
        PasteStrategy::MenuPaste => &[
            PasteStrategy::AppleScriptCmdV,
            PasteStrategy::AxInsert,
            PasteStrategy::CgEventCmdV,
        ],
        PasteStrategy::AppleScriptCmdV => &[PasteStrategy::AxInsert, PasteStrategy::CgEventCmdV],
        PasteStrategy::TypeText => &[PasteStrategy::AxInsert],
        PasteStrategy::AxInsert => &[PasteStrategy::AppleScriptCmdV],
        PasteStrategy::CgEventCmdV => &[PasteStrategy::AxInsert],
    }
}

impl fmt::Display for PasteStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PasteStrategy::MenuPaste => "MenuPaste",
            PasteStrategy::AppleScriptCmdV => "AppleScriptCmdV",
            PasteStrategy::CgEventCmdV => "CgEventCmdV",
            PasteStrategy::TypeText => "TypeText",
            PasteStrategy::AxInsert => "AxInsert",
        })
    }
}

/// Escape a string for embedding in an AppleScript `"..."`.
pub fn escape_applescript(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn menu_paste_script() -> &'static str {
    r#"tell application "System Events"
    tell process (name of first application process whose frontmost is true)
        tell (menu item "Paste" of menu of menu item "Paste" of menu "Edit" of menu bar item "Edit" of menu bar 1)
            if exists then
                if enabled then
                    click it
                    return true
                else
                    return false
                end if
            end if
        end tell
        tell (menu item "Paste" of menu "Edit" of menu bar item "Edit" of menu bar 1)
            if exists then
                if enabled then
                    click it
                    return true
                else
                    return false
                end if
            else
                return false
            end if
        end tell
    end tell
end tell"#
}

pub fn cmd_v_script() -> &'static str {
    r#"tell application "System Events" to keystroke "v" using command down"#
}

pub fn type_text_script(text: &str) -> String {
    format!(
        r#"tell application "System Events" to keystroke "{}""#,
        escape_applescript(text)
    )
}

pub fn frontmost_bundle_script() -> &'static str {
    r#"tell application "System Events" to get bundle identifier of first application process whose frontmost is true"#
}

/// TCC copy for paste (Accessibility + Automation of System Events).
pub const PASTE_TCC_HELP: &str = "\
Pasting into the app that was frontmost when you pressed the hotkey needs:

1. Accessibility — System Settings → Privacy & Security → Accessibility
   (AX insert and CGEvent). Enable **lailaisay.app** when you launched the
   packaged bundle; if you `cargo run`, enable Terminal / iTerm / Cursor
   and lailaisay-app if it appears.
2. Automation — System Events (Edit → Paste / keystroke). Error 1002 means
   this was denied. Also allow controlling the target app if macOS asks.

lailaisay restores that app before pasting (Smart/Ollama can take a minute).
CGEvent ⌘V is not treated as success unless the focused field changes.
If every strategy fails, the transcription stays on the clipboard — Cmd+V.
No API keys are used.
";

/// Windows paste notes (SendInput; no UI Automation in v1).
pub const WINDOWS_PASTE_HELP: &str = "\
lailaisay copies the transcript, then injects Ctrl+V via SendInput \
(Shift+Insert if that fails). The HWND focused at hotkey-down is restored \
with SetForegroundWindow + AttachThreadInput — Windows can still refuse \
focus after a long enhance. Success means SendInput accepted the events; \
v1 does not use UI Automation to prove the field changed. If injection \
fails, the text stays on the clipboard — Ctrl+V. No API keys.
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electron_skips_menu() {
        assert_eq!(
            choose_strategy("com.microsoft.VSCode", true),
            PasteStrategy::AppleScriptCmdV
        );
        assert_eq!(
            choose_strategy("com.google.Chrome", true),
            PasteStrategy::AppleScriptCmdV
        );
        assert_eq!(
            choose_strategy("com.apple.TextEdit", true),
            PasteStrategy::MenuPaste
        );
    }

    #[test]
    fn typing_when_clipboard_paste_off() {
        assert_eq!(
            choose_strategy("com.apple.TextEdit", false),
            PasteStrategy::TypeText
        );
    }

    #[test]
    fn applescript_escape() {
        assert_eq!(escape_applescript(r#"say "hi""#), r#"say \"hi\""#);
    }

    #[test]
    fn type_script_contains_escaped_text() {
        let s = type_text_script(r#"foo"bar"#);
        assert!(s.contains(r#"foo\"bar"#), "{s}");
    }

    #[test]
    fn fallbacks_after_menu_include_cgevent_and_ax() {
        let fb = fallback_after(PasteStrategy::MenuPaste);
        assert!(fb.contains(&PasteStrategy::CgEventCmdV));
        assert!(fb.contains(&PasteStrategy::AxInsert));
        assert!(fb.contains(&PasteStrategy::AppleScriptCmdV));
        let as_pos = fb
            .iter()
            .position(|s| *s == PasteStrategy::AppleScriptCmdV)
            .unwrap();
        let ax_pos = fb
            .iter()
            .position(|s| *s == PasteStrategy::AxInsert)
            .unwrap();
        let cg_pos = fb
            .iter()
            .position(|s| *s == PasteStrategy::CgEventCmdV)
            .unwrap();
        assert!(as_pos < cg_pos && ax_pos < cg_pos);
    }

    #[test]
    fn ax_verify_requires_change_and_needle() {
        assert!(!ax_value_shows_paste(Some("hello"), Some("hello"), "hello"));
        assert!(ax_value_shows_paste(
            Some("hello"),
            Some("hello world"),
            "world"
        ));
        assert!(!ax_value_shows_paste(None, None, "x"));
        assert!(ax_value_shows_paste(None, Some("去台南。"), "去台南。"));
    }

    #[test]
    fn script_ok_ax_unchanged_must_not_second_cmd_v_on_terminals() {
        let text = "看起來這是一個不怎麼活躍的社群。";
        let prompt = "user@host % ";
        for bundle in [
            "com.apple.Terminal",
            "com.googlecode.iterm2",
            "dev.warp.Warp-Stable",
            "dev.warp.Warp",
            "com.mitchellh.ghostty",
            "org.alacritty",
            "io.alacritty",
            "net.kovidgoyal.kitty",
            "com.todesktop.230313mzl4w4u92",
            "com.cursor.Cursor",
        ] {
            assert!(
                is_ax_flaky_terminal_bundle(bundle),
                "expected flaky terminal AX: {bundle}"
            );
            assert!(
                !should_attempt_second_clipboard_paste(bundle, Some(prompt), Some(prompt), text),
                "script ok + AX unchanged must not attempt second Cmd+V: {bundle}"
            );
        }
    }

    #[test]
    fn script_ok_ax_unchanged_retries_only_when_reliable_field_lacks_text() {
        assert!(should_attempt_second_clipboard_paste(
            "com.apple.TextEdit",
            Some("hello"),
            Some("hello"),
            "world"
        ));
        assert!(should_attempt_second_clipboard_paste(
            "com.apple.TextEdit",
            Some(""),
            Some(""),
            "world"
        ));
    }

    #[test]
    fn script_ok_does_not_retry_when_before_already_has_needle() {
        let text = "看起來這是一個不怎麼活躍的社群。";
        assert!(!should_attempt_second_clipboard_paste(
            "com.apple.TextEdit",
            Some(text),
            Some(text),
            text
        ));
    }

    #[test]
    fn script_ok_no_ax_trusts_script() {
        assert!(!should_attempt_second_clipboard_paste(
            "com.apple.TextEdit",
            None,
            None,
            "hello"
        ));
    }

    #[test]
    fn script_ok_ax_growth_does_not_retry() {
        assert!(!should_attempt_second_clipboard_paste(
            "com.apple.TextEdit",
            Some("hello"),
            Some("hello world"),
            "world"
        ));
        assert!(!should_attempt_second_clipboard_paste(
            "com.apple.Terminal",
            Some("user@host % "),
            Some("user@host % 看起來這是一個不怎麼活躍的社群。"),
            "看起來這是一個不怎麼活躍的社群。"
        ));
    }

    #[test]
    fn ordinary_apps_are_not_flaky_terminals() {
        assert!(!is_ax_flaky_terminal_bundle("com.apple.TextEdit"));
        assert!(!is_ax_flaky_terminal_bundle("com.google.Chrome"));
        assert!(!is_ax_flaky_terminal_bundle("com.microsoft.VSCode"));
    }
}
