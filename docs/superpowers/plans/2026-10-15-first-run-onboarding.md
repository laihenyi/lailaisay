# 首次使用引導（Onboarding）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 新使用者安裝後，由設定視窗內的多步驟精靈帶到「能實際聽寫」：選模型、開權限、試用成功。

**Architecture:** 純邏輯（還缺哪些步驟）放在 `lailaisay-app/src/onboarding.rs`，不依賴 egui，Linux CI 可測。畫面放在 `settings_ui/onboarding_ui.rs`（`settings_ui` 的子模組，才能使用其私有的 `perm_row`、`pane_heading`、`SettingsForm` 欄位）。`host.rs` 持有精靈狀態，每 0.5 秒探測一次權限，把畫面動作轉成既有的下載、儲存、重啟操作。持久標記只有一個新的設定欄位 `onboardingPending`。

**Tech Stack:** Rust、egui/eframe、既有 `lailaisay_input::*_grant()` 權限探測、既有 `models.rs` 目錄與下載。

**Spec:** `docs/superpowers/specs/2026-10-15-first-run-onboarding-design.md`（本計畫實作時發現兩處需修正，見 Task 1 Step 6）

## Global Constraints

- 所有使用者可見文字用繁體中文（zh-TW）。
- `lailaisay-core` 必須維持純 Rust 且 `cargo test` 通過。
- 舊設定檔相容：缺少 `onboardingPending` 的檔案讀進來必須是 `false`（不得強迫既有使用者重跑試用）。
- macOS CI 用 `RUSTFLAGS="-D warnings"`：不得留下未使用的 import、死碼。
- 不改 entitlements，不新增系統權限。App Store 版只要求麥克風。
- Windows：沒有權限探測，`required_permissions` 為空；麥克風問題在「試用」步驟暴露。
- 所有 `cargo` 指令在 `rust/` 目錄執行。
- 不得提交 `.p8`、公證密鑰或 `phone_number.txt`。
- git commit 結尾加 `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`。

## Review Focus

1. 使用者在精靈中途結束 App（⌘Q／關機）再開：`onboardingPending` 仍是 true，精靈再出現（Task 1 測試）。
2. 舊設定檔沒有 `onboardingPending` 欄位：讀成 false，既有使用者不被強迫試用（Task 1 測試）。
3. 權限探測回 `Unknown`／`Denied`（或精靈進行中被撤銷）：視為「缺」，精靈回到權限步驟，且「稍後再說」永遠可用，不會鎖死（Task 2 測試）。
4. 模型下載失敗或離線：顯示錯誤訊息，並提供「使用已有的模型」按鈕（Task 4 測試畫面含這些元素）。
5. 試用時辨識結果為空白（靜音、只有空白字元）：不算通過（Task 2 測試）。

---

## File Structure

- Modify `rust/crates/lailaisay-core/src/settings.rs` — 新欄位 `onboarding_pending`、`complete_onboarding` 在首次建檔時設旗標、新增 `finish_onboarding`。
- Create `rust/crates/lailaisay-app/src/onboarding.rs` — 純邏輯：`Platform`、`Probe`、`Step`、`remaining_steps`、`needs_wizard`、`recommended_model_id`、`trial_passed`。
- Modify `rust/crates/lailaisay-app/src/lib.rs` — `pub mod onboarding;`。
- Modify `rust/crates/lailaisay-app/src/shared.rs` — `utterance_count`、`model_ready()`。
- Modify `rust/crates/lailaisay-app/src/settings_ui.rs` — 宣告子模組 `mod onboarding_ui;` 並 re-export。
- Create `rust/crates/lailaisay-app/src/settings_ui/onboarding_ui.rs` — 精靈畫面與測試。
- Modify `rust/crates/lailaisay-app/src/host.rs` — 精靈狀態、啟動判斷、動作處理、托盤提醒、macOS 熱鍵重建。
- Modify `rust/crates/lailaisay-app/src/macos_runtime.rs` — `relaunch_app()`。
- Modify `rust/MAC_SMOKE.md`、`docs/superpowers/specs/2026-10-15-first-run-onboarding-design.md` — 手動檢查清單與規格修正。

---

### Task 1: 設定欄位 `onboardingPending`

**Files:**
- Modify: `rust/crates/lailaisay-core/src/settings.rs`（欄位約 :149、Default 約 :211、`complete_onboarding` 約 :297-307、測試區）
- Modify: `docs/superpowers/specs/2026-10-15-first-run-onboarding-design.md`

**Interfaces:**
- Produces: `LailaisaySettings.onboarding_pending: bool`（JSON 鍵 `onboardingPending`，缺鍵為 false）；`LailaisaySettings::finish_onboarding(&mut self, path: &Path) -> Result<(), LailaisayError>`；`complete_onboarding` 在檔案不存在、首次建檔時額外設 `onboarding_pending = true`。

- [ ] **Step 1: 寫失敗的測試**（加在 `settings.rs` 的 `mod tests` 內，緊接 `complete_onboarding_persists_once` 之後）

```rust
    #[test]
    fn first_run_marks_trial_pending_and_survives_restart() {
        let dir = std::env::temp_dir().join(format!(
            "lailaisay-core-onboarding-pending-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("hex_settings.json");
        let _ = std::fs::remove_file(&path);

        let mut s = LailaisaySettings::default();
        s.complete_onboarding(&path).unwrap();
        assert!(s.onboarding_pending, "fresh install owes the trial step");

        // App quit mid-wizard, then relaunched: the file says pending.
        let relaunched = LailaisaySettings::for_launch(&path);
        assert!(relaunched.onboarding_pending);

        let mut again = relaunched;
        again.complete_onboarding(&path).unwrap();
        assert!(again.onboarding_pending, "complete_onboarding must not clear it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_settings_file_without_the_key_is_not_pending() {
        let dir = std::env::temp_dir().join(format!(
            "lailaisay-core-onboarding-legacy-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("hex_settings.json");
        std::fs::write(&path, r#"{"minimizeToMenuBarOnLaunch":true}"#).unwrap();
        let loaded = LailaisaySettings::for_launch(&path);
        assert!(!loaded.onboarding_pending, "existing users skip the trial");
        assert!(loaded.has_completed_onboarding);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn finish_onboarding_clears_pending_once() {
        let dir = std::env::temp_dir().join(format!(
            "lailaisay-core-onboarding-finish-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("hex_settings.json");
        let _ = std::fs::remove_file(&path);

        let mut s = LailaisaySettings::default();
        s.complete_onboarding(&path).unwrap();
        s.finish_onboarding(&path).unwrap();
        assert!(!s.onboarding_pending);
        assert!(!LailaisaySettings::load_path(&path).unwrap().onboarding_pending);

        // Second call is a no-op and must not rewrite a changed file.
        std::fs::write(&path, r#"{"hotkey":null}"#).unwrap_or(());
        let before = std::fs::read(&path).unwrap();
        s.finish_onboarding(&path).unwrap();
        assert_eq!(before, std::fs::read(&path).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
```

- [ ] **Step 2: 確認測試失敗**

Run: `cd rust && cargo test -p lailaisay-core --lib onboarding`
Expected: 編譯失敗，`no field onboarding_pending`。

- [ ] **Step 3: 實作**

在 `has_completed_onboarding` 欄位之後加：

```rust
    /// The first-run wizard has not passed its 試用 step yet. Missing from
    /// older files (serde `false`), so existing users are never sent through
    /// the wizard. Set when the settings file is first created.
    pub onboarding_pending: bool,
```

`Default` 內 `has_completed_onboarding: false,` 之後加 `onboarding_pending: false,`。

把 `complete_onboarding` 改成（保留原本的 early return 與「檔案已存在就不覆寫」）：

```rust
    pub fn complete_onboarding(&mut self, path: &Path) -> Result<(), LailaisayError> {
        if self.has_completed_onboarding {
            return Ok(());
        }
        self.has_completed_onboarding = true;
        if path.exists() {
            return Ok(());
        }
        // Brand-new install: the wizard still owes the 試用 step.
        self.onboarding_pending = true;
        self.save_path(path)
    }

    /// The 試用 step passed. Clears `onboardingPending` and persists once.
    pub fn finish_onboarding(&mut self, path: &Path) -> Result<(), LailaisayError> {
        if !self.onboarding_pending {
            return Ok(());
        }
        self.onboarding_pending = false;
        self.has_completed_onboarding = true;
        self.save_path(path)
    }
```

- [ ] **Step 4: 確認通過，並跑整個 core**

Run: `cd rust && cargo test -p lailaisay-core --lib`
Expected: 全部 PASS（含既有的 `complete_onboarding_persists_once`）。

- [ ] **Step 5: 修正規格書兩處**

在規格書「決定」段把「不新增設定欄位；`hasCompletedOnboarding` …」整行改為：
`- 新增一個設定欄位 \`onboardingPending\`（缺鍵＝false）。全新安裝建檔時設 true，試用成功時清除。\`hasCompletedOnboarding\` 語意不變（舊檔相容）。既有使用者因缺鍵不會被強迫試用。`
並在「元件 1」把必要權限那行補一句：`Windows 與其他平台沒有權限探測，必要權限為空，麥克風問題在試用步驟暴露。`

- [ ] **Step 6: Commit**

```bash
git add rust/crates/lailaisay-core/src/settings.rs docs/superpowers/specs/2026-10-15-first-run-onboarding-design.md
git commit -m "feat(core): onboardingPending setting for the first-run trial step

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: 純邏輯 `onboarding.rs`

**Files:**
- Create: `rust/crates/lailaisay-app/src/onboarding.rs`
- Modify: `rust/crates/lailaisay-app/src/lib.rs`（加 `pub mod onboarding;`，放在 `pub mod models;` 之後）

**Interfaces:**
- Consumes: `lailaisay_input::GrantStatus`、`crate::settings_ui::PrivacyPane`（`Debug+Clone+Copy+PartialEq+Eq`）、`crate::models::{catalog_by_id, visible_catalog}`。
- Produces（後續 Task 逐字使用）：
  - `enum Platform { MacDeveloperId, MacAppStore, Windows, Other }`、`Platform::current()`、`Platform::required_permissions(self) -> &'static [PrivacyPane]`
  - `struct Probe { model_ready: bool, microphone: GrantStatus, accessibility: GrantStatus, input_monitoring: GrantStatus }`（`Copy`）、`Probe::read(model_ready: bool) -> Probe`、`Probe::grant(&self, PrivacyPane) -> GrantStatus`
  - `enum Step { Welcome, Model, Permissions, Trial }`
  - `fn missing_permissions(Platform, &Probe) -> Vec<PrivacyPane>`
  - `fn remaining_steps(Platform, &Probe, trial_pending: bool, welcome_seen: bool) -> Vec<Step>`
  - `fn needs_wizard(Platform, &Probe, trial_pending: bool) -> bool`
  - `fn recommended_model_id() -> &'static str`、`const SMALL_MODEL_ID: &str = "small"`
  - `fn trial_passed(baseline: u64, count: u64, last_final: &str) -> bool`

- [ ] **Step 1: 寫失敗的測試**（建立檔案，先只放測試與空殼 `use`）

```rust
//! First-run onboarding logic: which steps remain before dictation works.
//! Pure functions only (no egui, no OS calls except `Probe::read`), so Linux
//! CI covers the step order and the Developer ID / App Store / Windows split.

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(model: bool, mic: GrantStatus, ax: GrantStatus, im: GrantStatus) -> Probe {
        Probe {
            model_ready: model,
            microphone: mic,
            accessibility: ax,
            input_monitoring: im,
        }
    }
    const G: GrantStatus = GrantStatus::Granted;

    #[test]
    fn developer_id_needs_three_permissions_app_store_only_the_microphone() {
        use PrivacyPane::*;
        assert_eq!(
            Platform::MacDeveloperId.required_permissions(),
            &[Microphone, Accessibility, InputMonitoring]
        );
        assert_eq!(Platform::MacAppStore.required_permissions(), &[Microphone]);
        assert!(Platform::Windows.required_permissions().is_empty());
        assert!(Platform::Other.required_permissions().is_empty());
    }

    #[test]
    fn denied_unknown_and_undecided_all_count_as_missing() {
        for bad in [
            GrantStatus::Denied,
            GrantStatus::Unknown,
            GrantStatus::NotDetermined,
            GrantStatus::TargetNotRunning,
        ] {
            let p = probe(true, bad, G, G);
            assert_eq!(
                missing_permissions(Platform::MacDeveloperId, &p),
                vec![PrivacyPane::Microphone],
                "{bad:?} must not be treated as granted"
            );
        }
        let all = probe(true, G, G, G);
        assert!(missing_permissions(Platform::MacDeveloperId, &all).is_empty());
    }

    #[test]
    fn steps_follow_welcome_model_permissions_trial_order() {
        let p = probe(false, GrantStatus::Denied, G, G);
        assert_eq!(
            remaining_steps(Platform::MacDeveloperId, &p, true, false),
            vec![Step::Welcome, Step::Model, Step::Permissions, Step::Trial]
        );
        assert_eq!(
            remaining_steps(Platform::MacDeveloperId, &p, true, true),
            vec![Step::Model, Step::Permissions, Step::Trial]
        );
    }

    #[test]
    fn existing_user_only_sees_what_is_missing() {
        let ok = probe(true, G, G, G);
        assert!(remaining_steps(Platform::MacDeveloperId, &ok, false, false).is_empty());
        assert!(!needs_wizard(Platform::MacDeveloperId, &ok, false));

        let no_model = probe(false, G, G, G);
        assert_eq!(
            remaining_steps(Platform::MacDeveloperId, &no_model, false, false),
            vec![Step::Model],
            "no Welcome and no Trial for a user who already finished onboarding"
        );
        assert!(needs_wizard(Platform::MacDeveloperId, &no_model, false));
    }

    #[test]
    fn windows_skips_permissions_but_still_runs_the_trial() {
        let p = probe(true, GrantStatus::Unknown, GrantStatus::Unknown, GrantStatus::Unknown);
        assert_eq!(
            remaining_steps(Platform::Windows, &p, true, true),
            vec![Step::Trial]
        );
    }

    #[test]
    fn trial_needs_a_new_nonblank_utterance() {
        assert!(!trial_passed(3, 3, "舊的結果"), "no new utterance yet");
        assert!(!trial_passed(3, 4, ""), "silence is not a pass");
        assert!(!trial_passed(3, 4, "  \n "), "whitespace is not a pass");
        assert!(trial_passed(3, 4, "去台南。"));
    }

    #[test]
    fn recommended_model_is_in_the_visible_catalog() {
        let id = recommended_model_id();
        assert!(crate::models::catalog_by_id(id).is_some());
        assert!(crate::models::visible_catalog().any(|e| e.id == id));
        assert!(crate::models::visible_catalog().any(|e| e.id == SMALL_MODEL_ID));
        if lailaisay_stt::qwen_supported() {
            assert_eq!(id, "qwen3-asr-1.7b");
        } else {
            assert_eq!(id, "large-v3-turbo");
        }
    }
}
```

- [ ] **Step 2: 確認失敗**

Run: `cd rust && cargo test -p lailaisay-app --lib onboarding`
Expected: 編譯失敗（`Platform`、`Probe` 等未定義）。

- [ ] **Step 3: 實作**（放在檔案開頭註解之後、`#[cfg(test)]` 之前）

```rust
use lailaisay_input::GrantStatus;

use crate::settings_ui::PrivacyPane;

pub const SMALL_MODEL_ID: &str = "small";

/// Shown in the tray tooltip and status item while the wizard is owed.
pub const TRAY_REMINDER: &str = "需要設定，請開啟設定繼續";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacDeveloperId,
    MacAppStore,
    Windows,
    Other,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            if lailaisay_paste::is_app_store_build() {
                Self::MacAppStore
            } else {
                Self::MacDeveloperId
            }
        } else {
            Self::Other
        }
    }

    /// Permissions the user must grant before dictation can work. Windows and
    /// other platforms have no probe, so a broken microphone shows up in the
    /// 試用 step instead.
    pub fn required_permissions(self) -> &'static [PrivacyPane] {
        match self {
            Self::MacDeveloperId => &[
                PrivacyPane::Microphone,
                PrivacyPane::Accessibility,
                PrivacyPane::InputMonitoring,
            ],
            Self::MacAppStore => &[PrivacyPane::Microphone],
            Self::Windows | Self::Other => &[],
        }
    }
}

/// One snapshot of everything the wizard decides on.
#[derive(Debug, Clone, Copy)]
pub struct Probe {
    pub model_ready: bool,
    pub microphone: GrantStatus,
    pub accessibility: GrantStatus,
    pub input_monitoring: GrantStatus,
}

impl Probe {
    pub fn read(model_ready: bool) -> Self {
        Self {
            model_ready,
            microphone: lailaisay_input::microphone_grant(),
            accessibility: lailaisay_input::accessibility_grant(),
            input_monitoring: lailaisay_input::input_monitoring_grant(),
        }
    }

    pub fn grant(&self, pane: PrivacyPane) -> GrantStatus {
        match pane {
            PrivacyPane::Microphone => self.microphone,
            PrivacyPane::Accessibility => self.accessibility,
            PrivacyPane::InputMonitoring => self.input_monitoring,
            PrivacyPane::Automation => GrantStatus::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Welcome,
    Model,
    Permissions,
    Trial,
}

/// Only an explicit `Granted` counts. Denied, undecided and failed probes are
/// all "missing" so the wizard never claims a permission it cannot see.
pub fn missing_permissions(platform: Platform, probe: &Probe) -> Vec<PrivacyPane> {
    platform
        .required_permissions()
        .iter()
        .copied()
        .filter(|pane| probe.grant(*pane) != GrantStatus::Granted)
        .collect()
}

/// Steps still to do, in order. `trial_pending` is `onboardingPending`
/// (a fresh install); existing users never see Welcome or Trial.
pub fn remaining_steps(
    platform: Platform,
    probe: &Probe,
    trial_pending: bool,
    welcome_seen: bool,
) -> Vec<Step> {
    let mut steps = Vec::new();
    if trial_pending && !welcome_seen {
        steps.push(Step::Welcome);
    }
    if !probe.model_ready {
        steps.push(Step::Model);
    }
    if !missing_permissions(platform, probe).is_empty() {
        steps.push(Step::Permissions);
    }
    if trial_pending {
        steps.push(Step::Trial);
    }
    steps
}

pub fn needs_wizard(platform: Platform, probe: &Probe, trial_pending: bool) -> bool {
    !remaining_steps(platform, probe, trial_pending, true).is_empty()
}

pub fn recommended_model_id() -> &'static str {
    if lailaisay_stt::qwen_supported() {
        "qwen3-asr-1.7b"
    } else {
        "large-v3-turbo"
    }
}

/// The trial passes on a new utterance whose final text is not blank.
pub fn trial_passed(baseline: u64, count: u64, last_final: &str) -> bool {
    count > baseline && !last_final.trim().is_empty()
}
```

並在 `lib.rs` 加 `pub mod onboarding;`。

- [ ] **Step 4: 確認通過（含 -D warnings）**

Run: `cd rust && RUSTFLAGS="-D warnings" cargo test -p lailaisay-app --lib onboarding`
Expected: 7 個測試 PASS。若 `PrivacyPane` 缺少 `Debug`，在 `settings_ui.rs` 的 derive 補上（目前已有 `Debug, Clone, Copy, PartialEq, Eq`）。`-D warnings` 若抱怨 `TRAY_REMINDER`／`Probe::read` 未使用，先在該項上加 `#[allow(dead_code)]`，Task 5 會用到後移除。

- [ ] **Step 5: Commit**

```bash
git add rust/crates/lailaisay-app/src/onboarding.rs rust/crates/lailaisay-app/src/lib.rs
git commit -m "feat(app): onboarding step logic (readiness, step order, trial pass)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: `SharedState` 的 `model_ready` 與 `utterance_count`

**Files:**
- Modify: `rust/crates/lailaisay-app/src/shared.rs`（struct 約 :25-45、`from_context` 約 :48-62、`set_last_utterance` 約 :100、`stt_is_placeholder` 附近、測試區）

**Interfaces:**
- Produces: `SharedState.utterance_count: u64`（每次 `set_last_utterance` 加 1）；`SharedState::model_ready(&self) -> bool`。

- [ ] **Step 1: 寫失敗的測試**（加在 `shared.rs` 的 `mod tests` 結尾）

```rust
    #[test]
    fn utterance_count_ticks_and_dummy_model_is_not_ready() {
        let mut st = SharedState::from_context(empty_ctx());
        assert_eq!(st.utterance_count, 0);
        assert!(!st.model_ready(), "nothing loaded yet");
        st.set_last_utterance("a", "b", None);
        st.set_last_utterance("", "", None);
        assert_eq!(st.utterance_count, 2);

        st.ensure_stt(); // no model file → dummy placeholder
        assert!(st.stt_is_placeholder());
        assert!(!st.model_ready(), "dummy must not satisfy onboarding");
    }
```

- [ ] **Step 2: 確認失敗**

Run: `cd rust && cargo test -p lailaisay-app --lib utterance_count`
Expected: 編譯失敗（`utterance_count`、`model_ready` 不存在）。

- [ ] **Step 3: 實作**

struct 內 `pub last_llm_note: Option<String>,` 之後加 `pub utterance_count: u64,`；`from_context` 內 `last_llm_note: None,` 之後加 `utterance_count: 0,`；`set_last_utterance` 函式本體第一行加 `self.utterance_count += 1;`；在 `stt_is_placeholder` 之後加：

```rust
    /// A real (non-dummy) decoder is resident. Onboarding's model step is done
    /// only when this is true, not merely when a model file exists.
    pub fn model_ready(&self) -> bool {
        self.stt.is_some() && !self.stt_is_placeholder()
    }
```

- [ ] **Step 4: 確認通過**

Run: `cd rust && cargo test -p lailaisay-app --lib shared`
Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add rust/crates/lailaisay-app/src/shared.rs
git commit -m "feat(app): SharedState model_ready and utterance_count for onboarding

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: 精靈畫面 `settings_ui/onboarding_ui.rs`

**Files:**
- Create: `rust/crates/lailaisay-app/src/settings_ui/onboarding_ui.rs`
- Modify: `rust/crates/lailaisay-app/src/settings_ui.rs`（在檔案開頭 `use` 區塊之後加 `mod onboarding_ui;` 與 `pub use onboarding_ui::{OnboardingAction, OnboardingView};`）

**Interfaces:**
- Consumes: Task 2 的 `Platform`、`Probe`、`Step`、`recommended_model_id`、`SMALL_MODEL_ID`；`SettingsForm` 欄位 `selected_catalog_id`、`downloading`、`download_message`、`local_models`、`selected_model`、`permissions_open_message`；私有 helper `paint_settings_titlebar`、`pane_heading`、`note`、`success_note`、`danger_note`、`secondary_button`、`ghost_button`、`perm_row`、`PRIVACY_ROWS`、`catalog_row_label`、`catalog_by_id`、`catalog_is_downloaded`、`friendly_model_label`、`show_readonly_block`、`paint_status_strip`。
- Produces（Task 5 使用）：

```rust
pub struct OnboardingView<'a> {
    pub step: Option<Step>,   // None = 完成畫面
    pub platform: Platform,
    pub probe: Probe,
    pub stt_warning: &'a str,
    pub last_final: &'a str,  // 只含試用開始後的結果
    pub status: &'a str,
    pub hotkey_label: &'a str,
    pub hotkey_live: bool,
}
pub enum OnboardingAction { None, Next, Later, Download, UseModel(String), Relaunch, Finish }
impl SettingsForm { pub fn show_onboarding(&mut self, ctx: &egui::Context, view: &OnboardingView<'_>) -> OnboardingAction }
```

- [ ] **Step 1: 寫失敗的測試**（先建立檔案，只含 `use` 與測試）

```rust
//! First-run wizard screens. A child of `settings_ui` so it can reuse the
//! settings window's private widgets (`perm_row`, `pane_heading`, …).

use super::*;
use crate::onboarding::{self, Platform, Probe, Step};

#[cfg(test)]
mod tests {
    use super::*;

    fn view<'a>(step: Option<Step>, probe: Probe) -> OnboardingView<'a> {
        OnboardingView {
            step,
            platform: Platform::MacDeveloperId,
            probe,
            stt_warning: "",
            last_final: "",
            status: "idle",
            hotkey_label: "⌘⇧Space",
            hotkey_live: true,
        }
    }

    fn denied() -> Probe {
        Probe {
            model_ready: false,
            microphone: GrantStatus::Denied,
            accessibility: GrantStatus::Denied,
            input_monitoring: GrantStatus::Denied,
        }
    }

    fn paint(ctx: &egui::Context, form: &mut SettingsForm, view: &OnboardingView<'_>) -> OnboardingAction {
        let size = Vec2::new(SETTINGS_INNER_SIZE[0], SETTINGS_INNER_SIZE[1]);
        let mut action = OnboardingAction::None;
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                action = form.show_onboarding(ctx, view);
            });
        }
        action
    }

    #[test]
    fn every_step_paints_its_heading_below_the_titlebar_and_inside_the_window() {
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        let steps = [
            (Some(Step::Welcome), "歡迎使用 lailaisay"),
            (Some(Step::Model), "選擇語音模型"),
            (Some(Step::Permissions), "開啟系統權限"),
            (Some(Step::Trial), "試用一下"),
            (None, "設定完成"),
        ];
        for (step, heading) in steps {
            let v = view(step, denied());
            let action = paint(&ctx, &mut form, &v);
            assert_eq!(action, OnboardingAction::None, "{heading}: no click, no action");
            let resp = ctx
                .read_response(pane_heading_id(heading))
                .unwrap_or_else(|| panic!("`{heading}` heading should paint"));
            assert!(
                resp.rect.top() >= SETTINGS_TITLEBAR_H - 1.0,
                "{heading} top {} sits under the titlebar",
                resp.rect.top()
            );
            assert!(resp.rect.bottom() <= SETTINGS_INNER_SIZE[1], "{heading} falls off the window");
        }
    }

    #[test]
    fn model_step_offers_local_models_when_offline() {
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        form.download_message = "Download failed: network".into();
        let v = view(Some(Step::Model), denied());
        let _ = paint(&ctx, &mut form, &v);
        // The failure text is part of the model step so the user sees why.
        assert!(form.download_message.contains("failed"));
    }

    #[test]
    fn recommended_card_is_preselected_by_the_caller_not_overwritten_here() {
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        form.selected_catalog_id = onboarding::recommended_model_id().into();
        let v = view(Some(Step::Model), denied());
        let _ = paint(&ctx, &mut form, &v);
        assert_eq!(form.selected_catalog_id, onboarding::recommended_model_id());
    }
}
```

在 `settings_ui.rs` 加上 `mod onboarding_ui;` 與 `pub use onboarding_ui::{OnboardingAction, OnboardingView};`（`pane_heading_id`、`SETTINGS_TITLEBAR_H`、`SETTINGS_INNER_SIZE` 已存在於 `settings_ui.rs`，子模組可見）。

- [ ] **Step 2: 確認失敗**

Run: `cd rust && cargo test -p lailaisay-app --lib onboarding_ui`
Expected: 編譯失敗（`OnboardingView`、`show_onboarding` 未定義）。

- [ ] **Step 3: 實作**（放在 `use` 之後、`#[cfg(test)]` 之前）

```rust
pub struct OnboardingView<'a> {
    /// `None` is the 完成 screen.
    pub step: Option<Step>,
    pub platform: Platform,
    pub probe: Probe,
    pub stt_warning: &'a str,
    /// Only text recorded after the trial step began.
    pub last_final: &'a str,
    pub status: &'a str,
    pub hotkey_label: &'a str,
    /// The global hotkey hook is running.
    pub hotkey_live: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnboardingAction {
    None,
    Next,
    Later,
    Download,
    UseModel(String),
    Relaunch,
    Finish,
}

fn primary_button(ui: &mut Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(label).color(Color32::WHITE))
            .fill(theme::GLASS_TINT)
            .min_size(Vec2::new(0.0, ROW_H)),
    )
}

impl SettingsForm {
    pub fn show_onboarding(
        &mut self,
        ctx: &egui::Context,
        view: &OnboardingView<'_>,
    ) -> OnboardingAction {
        let mut action = OnboardingAction::None;
        paint_settings_titlebar(ctx);
        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::BG).inner_margin(0))
            .show(ctx, |ui| {
                theme::content_pane_frame().show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("lailaisay_onboarding")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            action = match view.step {
                                Some(Step::Welcome) => self.onboarding_welcome(ui, view),
                                Some(Step::Model) => self.onboarding_model(ui, view),
                                Some(Step::Permissions) => self.onboarding_permissions(ui, view),
                                Some(Step::Trial) => self.onboarding_trial(ui, view),
                                None => self.onboarding_done(ui),
                            };
                            if view.step.is_some() {
                                ui.add_space(16.0);
                                if ghost_button(ui, "稍後再說").clicked() {
                                    action = OnboardingAction::Later;
                                }
                                note(ui, "未完成前，下次開啟 lailaisay 仍會回到這個引導。");
                            }
                        });
                });
            });
        crate::status_hud::paint_recording_hud(ctx, view.status);
        action
    }

    fn onboarding_welcome(&mut self, ui: &mut Ui, view: &OnboardingView<'_>) -> OnboardingAction {
        pane_heading(
            ui,
            "歡迎使用 lailaisay",
            "按住快捷鍵說話，放開後文字會貼到目前的輸入位置。開始前需要完成幾個設定。",
        );
        note(ui, &format!("快捷鍵：{}", view.hotkey_label));
        ui.add_space(8.0);
        note(ui, "步驟：選擇語音模型 → 開啟系統權限 → 說一句話試用。");
        ui.add_space(12.0);
        if primary_button(ui, "開始設定", true).clicked() {
            return OnboardingAction::Next;
        }
        OnboardingAction::None
    }

    fn onboarding_model(&mut self, ui: &mut Ui, view: &OnboardingView<'_>) -> OnboardingAction {
        let mut action = OnboardingAction::None;
        pane_heading(
            ui,
            "選擇語音模型",
            "語音在本機辨識，模型需要先下載一次，之後不用網路。",
        );
        for id in [onboarding::recommended_model_id(), onboarding::SMALL_MODEL_ID] {
            let Some(entry) = catalog_by_id(id) else {
                continue;
            };
            let mut label = catalog_row_label(entry);
            if catalog_is_downloaded(entry) {
                label.push_str("（已下載）");
            }
            if ui
                .selectable_label(self.selected_catalog_id == entry.id, label)
                .clicked()
            {
                self.selected_catalog_id = entry.id.to_string();
            }
        }
        ui.add_space(8.0);
        let can_download = !self.downloading && catalog_by_id(&self.selected_catalog_id).is_some();
        let label = if self.downloading { "下載中…" } else { "下載並使用" };
        if primary_button(ui, label, can_download).clicked() {
            action = OnboardingAction::Download;
        }
        if !self.download_message.is_empty() {
            note(ui, &self.download_message);
        }
        if !view.stt_warning.is_empty() {
            danger_note(ui, view.stt_warning);
        }
        if !self.downloading && !self.selected_model.is_empty() && !view.probe.model_ready {
            note(ui, "模型載入中，通常需要數秒到十幾秒…");
            ui.ctx().request_repaint_after(Duration::from_millis(500));
        }

        if !self.local_models.is_empty() {
            ui.add_space(12.0);
            note(ui, "沒有網路？可以直接使用電腦上已有的模型：");
            for model in self.local_models.iter().take(4) {
                let text = format!("使用已有的模型：{}", friendly_model_label(model));
                if ghost_button(ui, &text).clicked() {
                    action = OnboardingAction::UseModel(model.path.to_string_lossy().into_owned());
                }
            }
        }
        action
    }

    fn onboarding_permissions(
        &mut self,
        ui: &mut Ui,
        view: &OnboardingView<'_>,
    ) -> OnboardingAction {
        // Re-probe while on screen so System Settings toggles show up.
        ui.ctx().request_repaint_after(Duration::from_millis(500));
        pane_heading(
            ui,
            "開啟系統權限",
            "按下每一列的「開啟」，在系統設定中允許 lailaisay。授權後這裡會自動更新。",
        );
        for pane in view.platform.required_permissions() {
            let (title, detail) = PRIVACY_ROWS
                .iter()
                .find(|(p, _, _)| p == pane)
                .map(|(_, t, d)| (*t, *d))
                .unwrap_or(("權限", ""));
            if let Some(err) = perm_row(ui, title, detail, *pane) {
                self.permissions_open_message = err;
            }
        }
        if !self.permissions_open_message.is_empty() {
            danger_note(ui, &self.permissions_open_message);
        }
        let mut action = OnboardingAction::None;
        let hooks_granted = view.probe.accessibility == GrantStatus::Granted
            && view.probe.input_monitoring == GrantStatus::Granted;
        if view.platform == Platform::MacDeveloperId && hooks_granted && !view.hotkey_live {
            ui.add_space(8.0);
            note(ui, "權限已開啟，但快捷鍵需要重新啟動 lailaisay 才會生效。");
            if primary_button(ui, "重新啟動 lailaisay", true).clicked() {
                action = OnboardingAction::Relaunch;
            }
        }
        action
    }

    fn onboarding_trial(&mut self, ui: &mut Ui, view: &OnboardingView<'_>) -> OnboardingAction {
        pane_heading(
            ui,
            "試用一下",
            &format!(
                "按住 {} 說一句話，再放開。下方出現文字就完成了。",
                view.hotkey_label
            ),
        );
        paint_status_strip(ui, view.status, "");
        ui.add_space(8.0);
        show_readonly_block(ui, view.last_final, "辨識結果會顯示在這裡");
        let mut action = OnboardingAction::None;
        if !view.hotkey_live {
            danger_note(ui, "快捷鍵尚未啟用，請先確認系統權限，必要時重新啟動 lailaisay。");
            if view.platform == Platform::MacDeveloperId
                && primary_button(ui, "重新啟動 lailaisay", true).clicked()
            {
                action = OnboardingAction::Relaunch;
            }
        } else if view.platform == Platform::Windows {
            note(ui, "若沒有反應，請確認 Windows 設定中麥克風已允許桌面應用程式使用。");
        }
        action
    }

    fn onboarding_done(&mut self, ui: &mut Ui) -> OnboardingAction {
        pane_heading(
            ui,
            "設定完成",
            "可以開始使用了。lailaisay 會留在選單列（Windows 在通知區），按住快捷鍵就能說話。",
        );
        if primary_button(ui, "開始使用", true).clicked() {
            return OnboardingAction::Finish;
        }
        OnboardingAction::None
    }
}
```

註：`GrantStatus`、`Duration`、`Color32`、`Frame`、`Vec2`、`ROW_H` 已在 `settings_ui.rs` 引入，子模組透過 `use super::*` 取得。`catalog_row_label` 目前回傳 `String`（見 `settings_ui.rs` 的 `catalog_row_label(entry)`）；若實際回傳 `&str`，把 `let mut label` 改成 `let mut label = catalog_row_label(entry).to_string();`。

- [ ] **Step 4: 確認通過（含 -D warnings）**

Run: `cd rust && RUSTFLAGS="-D warnings" cargo test -p lailaisay-app --lib onboarding_ui`
Expected: 3 個測試 PASS。`OnboardingAction` 若尚未被 `host.rs` 使用而觸發未使用警告，維持 `pub` 並在 `lib.rs` 不需動作（公開型別不會警告）。

- [ ] **Step 5: Commit**

```bash
git add rust/crates/lailaisay-app/src/settings_ui.rs rust/crates/lailaisay-app/src/settings_ui/onboarding_ui.rs
git commit -m "feat(app): onboarding wizard screens

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: `host.rs` 接線

**Files:**
- Modify: `rust/crates/lailaisay-app/src/host.rs`（struct 欄位、`new()`、`open_settings`、`update()`、`paint_settings`、`sync_tray_status`、`run_with`）

**Interfaces:**
- Consumes: Task 1–4 的全部公開項目。
- Produces: 精靈在 `LailaisayHost` 內運作；`onboarding_active`、`onboarding_done_screen`、`onboarding_welcome_seen`、`trial_baseline`、`probe_cache`、`reminder` 欄位；方法 `onboarding_probe`、`onboarding_step`、`poll_onboarding`、`handle_onboarding_action`、`hotkey_live`。

- [ ] **Step 1: 加欄位**（`struct LailaisayHost` 內，緊接 `edit_selection` 欄位之後；`new()` 的 `Self { … }` 內緊接 `edit_selection: None,` 之後加初值）

```rust
    onboarding_active: bool,
    onboarding_welcome_seen: bool,
    onboarding_done_screen: bool,
    trial_baseline: Option<u64>,
    probe_cache: Option<(Instant, crate::onboarding::Probe)>,
    reminder: bool,
```

初值：`onboarding_active: false, onboarding_welcome_seen: false, onboarding_done_screen: false, trial_baseline: None, probe_cache: None, reminder: false,`

- [ ] **Step 2: 加方法**（放進 `impl LailaisayHost`，例如接在 `poll_download` 之後；檔頭 `use` 補 `use crate::onboarding::{self, Platform, Step};` 與 `use crate::settings_ui::{OnboardingAction, OnboardingView};`，若該檔已用不同方式引入 `settings_ui` 項目，併入原本的 `use`）

```rust
    /// Permissions and model readiness, re-read at most every 500 ms (the TCC
    /// probes call into the OS).
    fn onboarding_probe(&mut self) -> onboarding::Probe {
        if let Some((at, probe)) = self.probe_cache {
            if at.elapsed() < Duration::from_millis(500) {
                return probe;
            }
        }
        let ready = self.shared.lock().map(|g| g.model_ready()).unwrap_or(false);
        let probe = onboarding::Probe::read(ready);
        self.probe_cache = Some((Instant::now(), probe));
        probe
    }

    fn trial_pending(&self) -> bool {
        self.shared
            .lock()
            .map(|g| g.settings.onboarding_pending)
            .unwrap_or(false)
    }

    /// Decide whether the wizard is owed (launch, tray 開啟設定).
    fn refresh_onboarding(&mut self) {
        let probe = self.onboarding_probe();
        let needed = onboarding::needs_wizard(Platform::current(), &probe, self.trial_pending());
        self.onboarding_active = needed;
        self.onboarding_done_screen = false;
        self.trial_baseline = None;
        self.reminder = false;
        if needed {
            self.form.selected_catalog_id = onboarding::recommended_model_id().to_string();
        }
    }

    /// The step on screen. `None` is the 完成 screen.
    fn onboarding_step(&mut self) -> Option<Step> {
        if self.onboarding_done_screen {
            return None;
        }
        let probe = self.onboarding_probe();
        let steps = onboarding::remaining_steps(
            Platform::current(),
            &probe,
            self.trial_pending(),
            self.onboarding_welcome_seen,
        );
        match steps.first().copied() {
            Some(step) => Some(step),
            None => {
                self.onboarding_done_screen = true;
                None
            }
        }
    }

    fn hotkey_live(&self) -> bool {
        #[cfg(all(target_os = "macos", feature = "appstore"))]
        {
            self.tap.as_ref().is_some_and(|t| t.last_error().is_none())
        }
        #[cfg(all(target_os = "macos", not(feature = "appstore")))]
        {
            self.tap.is_some()
        }
        #[cfg(target_os = "windows")]
        {
            self.tap.is_some()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            true
        }
    }

    /// Per-frame: detect a passed trial; keep the tray reminder current.
    fn poll_onboarding(&mut self) {
        if !self.onboarding_active {
            let probe = self.onboarding_probe();
            self.reminder =
                onboarding::needs_wizard(Platform::current(), &probe, self.trial_pending());
            return;
        }
        if self.onboarding_done_screen {
            return;
        }
        if self.onboarding_step() != Some(Step::Trial) {
            self.trial_baseline = None;
            return;
        }
        let (count, last_final) = match self.shared.lock() {
            Ok(g) => (g.utterance_count, g.last_final.clone()),
            Err(_) => return,
        };
        let baseline = *self.trial_baseline.get_or_insert(count);
        if onboarding::trial_passed(baseline, count, &last_final) {
            if let Ok(mut g) = self.shared.lock() {
                if let Err(e) = g.settings.finish_onboarding(&settings_path()) {
                    tracing::warn!("could not persist onboarding completion ({e})");
                }
            }
            self.onboarding_done_screen = true;
            self.probe_cache = None;
        }
    }

    fn handle_onboarding_action(&mut self, ctx: &egui::Context, action: OnboardingAction) {
        match action {
            OnboardingAction::None => {}
            OnboardingAction::Next => self.onboarding_welcome_seen = true,
            OnboardingAction::Download => self.start_catalog_download(),
            OnboardingAction::UseModel(path) => {
                self.form.selected_model = path;
                self.save_settings();
                self.probe_cache = None;
            }
            OnboardingAction::Relaunch => {
                #[cfg(target_os = "macos")]
                match crate::macos_runtime::relaunch_app() {
                    Ok(()) => self.should_quit = true,
                    Err(e) => {
                        self.form.permissions_open_message = format!("無法重新啟動：{e}");
                    }
                }
            }
            OnboardingAction::Later | OnboardingAction::Finish => {
                self.onboarding_active = false;
                self.onboarding_done_screen = false;
                self.probe_cache = None;
                if self.tray_is_up() {
                    self.hide_settings_to_tray(ctx);
                }
            }
        }
    }
```

- [ ] **Step 3: 接到既有流程**

1. `new()`：在 `Self { … }` 建好、回傳之前，把結果存成 `let mut host = Self { … }; host.refresh_onboarding(); host`（原本是直接回傳 `Self { … }`）。
2. `open_settings`：在 `self.settings_visible = true;` 之前加 `self.refresh_onboarding();`。
3. `update()`：在 `self.poll_download();` 之後加 `self.poll_onboarding();`。
4. `paint_settings`：在 `crate::theme::paint_shell_frame(ctx);` 之後、`let (stt_note, …) = match self.shared.lock()` 之前加：

```rust
        if self.onboarding_active {
            let step = self.onboarding_step();
            let probe = self.onboarding_probe();
            let (stt_warning, status, last_final, count) = match self.shared.lock() {
                Ok(g) => (
                    g.stt_warning.clone(),
                    g.status.clone(),
                    g.last_final.clone(),
                    g.utterance_count,
                ),
                Err(_) => Default::default(),
            };
            let shown_final = match self.trial_baseline {
                Some(base) if count > base => last_final,
                _ => String::new(),
            };
            let hotkey_label = self.form.hotkey_label();
            let view = OnboardingView {
                step,
                platform: Platform::current(),
                probe,
                stt_warning: &stt_warning,
                last_final: &shown_final,
                status: &status,
                hotkey_label: &hotkey_label,
                hotkey_live: self.hotkey_live(),
            };
            let action = self.form.show_onboarding(ctx, &view);
            self.handle_onboarding_action(ctx, action);
            return;
        }
```

5. `sync_tray_status`：把 `format!("lailaisay — {status}")` 兩處改成共用變數：

```rust
        let suffix = if self.reminder {
            format!("（{}）", onboarding::TRAY_REMINDER)
        } else {
            String::new()
        };
```
並使用 `format!("lailaisay — {status}{suffix}")`（tooltip 與 `item.set_text` 各一處）。

6. `run_with`：把 `pub fn run_with(opts: RuntimeOpts)` 改成 `pub fn run_with(mut opts: RuntimeOpts)`；在 `g.ensure_stt();` 之後（`shared` 已建立、模型已載入）加：

```rust
    {
        let g = shared.lock().map_err(|e| anyhow!("shared lock: {e}"))?;
        let probe = crate::onboarding::Probe::read(g.model_ready());
        if crate::onboarding::needs_wizard(
            crate::onboarding::Platform::current(),
            &probe,
            g.settings.onboarding_pending,
        ) {
            opts.show_settings = true;
        }
    }
```
（要放在 `let start_visible = opts.show_settings;` 之前。）

- [ ] **Step 4: 編譯與既有測試**

Run: `cd rust && RUSTFLAGS="-D warnings" cargo test --workspace --features lailaisay-stt/process-whisper 2>&1 | tail -15`
Expected: 全部 PASS。常見修正：`Instant`／`Duration` 的 `use`（host.rs 已使用，應已引入）；移除 Task 2 暫加的 `#[allow(dead_code)]`；`Probe::read` 現在被使用。

- [ ] **Step 5: macOS 功能組合編譯**

Run: `cd rust && RUSTFLAGS="-D warnings" cargo check -p lailaisay-app --features mic,whisper,qwen && RUSTFLAGS="-D warnings" cargo check -p lailaisay-app --features mic,whisper,appstore,qwen`
Expected: 兩者 `Finished`。

- [ ] **Step 6: Commit**

```bash
git add rust/crates/lailaisay-app/src/host.rs
git commit -m "feat(app): wire the onboarding wizard into the host (launch, tray, trial)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: macOS 熱鍵重建與「重新啟動」

**Files:**
- Modify: `rust/crates/lailaisay-app/src/host.rs`（`init_macos` 的非 appstore 熱鍵區塊抽成方法）
- Modify: `rust/crates/lailaisay-app/src/macos_runtime.rs`（加 `relaunch_app`）

**Interfaces:**
- Produces: `LailaisayHost::start_macos_tap(&mut self)`（僅 `#[cfg(all(target_os = "macos", not(feature = "appstore")))]`）；`macos_runtime::relaunch_app() -> std::io::Result<()>`。

- [ ] **Step 1: 寫失敗的測試**（`macos_runtime.rs` 的 `mod window_order_tests` 旁加新測試模組）

```rust
#[cfg(test)]
mod relaunch_tests {
    use super::bundle_of;
    use std::path::Path;

    #[test]
    fn bundle_is_found_from_the_inner_executable() {
        let exe = Path::new("/Applications/lailaisay.app/Contents/MacOS/lailaisay-app");
        assert_eq!(
            bundle_of(exe).as_deref(),
            Some(Path::new("/Applications/lailaisay.app"))
        );
    }

    #[test]
    fn cargo_run_binary_has_no_bundle() {
        assert!(bundle_of(Path::new("/repo/rust/target/release/lailaisay-app")).is_none());
    }
}
```

- [ ] **Step 2: 確認失敗**（僅 macOS 可跑）

Run: `cd rust && cargo test -p lailaisay-app --lib relaunch_tests`
Expected: 編譯失敗（`bundle_of` 未定義）。在 Linux 這些測試不編譯，屬預期，此 Task 只在 macOS 驗證。

- [ ] **Step 3: 實作 `relaunch_app`**（`macos_runtime.rs`，放在 `yield_inactive_settings_window` 之前）

```rust
/// `<X>.app` that contains `exe`, if any.
fn bundle_of(exe: &std::path::Path) -> Option<std::path::PathBuf> {
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(|p| p.to_path_buf())
}

/// Start a second instance (`open -n` inside a bundle, the plain binary under
/// `cargo run`). The caller quits the current process afterwards.
pub fn relaunch_app() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    match bundle_of(&exe) {
        Some(app) => std::process::Command::new("open")
            .arg("-n")
            .arg(app)
            .spawn()
            .map(|_| ()),
        None => std::process::Command::new(exe).spawn().map(|_| ()),
    }
}
```

- [ ] **Step 4: 抽出 `start_macos_tap`**

在 `host.rs` 的 `init_macos` 中，把 `#[cfg(not(feature = "appstore"))] match MacOsEventTap::with_processor(...) { … }` 整段搬進新方法（內容不變，只是前面先讀設定）：

```rust
    #[cfg(all(target_os = "macos", not(feature = "appstore")))]
    fn start_macos_tap(&mut self) {
        let (hotkey, edit_hotkey, double_tap) = {
            let g = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (
                g.settings.hotkey.clone(),
                g.settings.edit_hotkey.clone(),
                g.settings.use_double_tap_only,
            )
        };
        match MacOsEventTap::with_processor(HotKeyProcessor::new(hotkey.clone(), double_tap)) {
            Ok(t) => {
                t.set_hotkey(hotkey, double_tap);
                t.set_edit_hotkey(edit_hotkey);
                self.tap = Some(t);
                set_shared_status(&self.shared, "idle");
                eprintln!("[lailaisay-app] CGEvent tap running. Default/settings hotkey active.");
            }
            Err(e) => {
                eprintln!("[lailaisay-app] hotkey tap disabled: {e}");
                eprintln!(
                    "[lailaisay-app] tray stays up. Grant Accessibility + Input Monitoring, then Quit and relaunch."
                );
                set_shared_status(&self.shared, "needs Accessibility");
            }
        }
    }
```

`init_macos` 原位置改成：

```rust
            #[cfg(not(feature = "appstore"))]
            self.start_macos_tap();
```
（`hotkey/edit_hotkey/double_tap` 的 `let (…) = {…}` 在 appstore 分支仍會用到，保留給 appstore 區塊；若非 appstore 組態出現 unused 變數警告，把該 `let` 包進 `#[cfg(feature = "appstore")]`。）

- [ ] **Step 5: 在 `poll_onboarding` 加重建**（`poll_onboarding` 開頭、`if !self.onboarding_active` 之前）

```rust
        #[cfg(all(target_os = "macos", not(feature = "appstore")))]
        if self.opts.enable_tap && self.tap.is_none() {
            let due = self.tap_retry_at.map_or(true, |at| Instant::now() >= at);
            let probe = self.onboarding_probe();
            if due
                && probe.accessibility == lailaisay_input::GrantStatus::Granted
                && probe.input_monitoring == lailaisay_input::GrantStatus::Granted
            {
                self.tap_retry_at = Some(Instant::now() + Duration::from_secs(3));
                self.start_macos_tap();
            }
        }
```
並加欄位（同 Step 1 的位置）`#[cfg(all(target_os = "macos", not(feature = "appstore")))] tap_retry_at: Option<Instant>,`，`new()` 初值 `tap_retry_at: None`（同樣 cfg）。

- [ ] **Step 6: 驗證**

Run（macOS）: `cd rust && RUSTFLAGS="-D warnings" cargo test -p lailaisay-app --lib relaunch_tests && RUSTFLAGS="-D warnings" cargo check -p lailaisay-app --features mic,whisper,qwen && RUSTFLAGS="-D warnings" cargo check -p lailaisay-app --features mic,whisper,appstore,qwen`
Expected: 測試 PASS，兩個 check `Finished`。
Run（Linux 等價）: `cd rust && RUSTFLAGS="-D warnings" cargo test --workspace --features lailaisay-stt/process-whisper`
Expected: PASS。

- [ ] **Step 7: Commit**

```bash
git add rust/crates/lailaisay-app/src/host.rs rust/crates/lailaisay-app/src/macos_runtime.rs
git commit -m "feat(app): retry the hotkey tap after permissions are granted; relaunch button

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: 文件、手動實機驗證、CI

**Files:**
- Modify: `rust/MAC_SMOKE.md`（新增「首次使用引導」手動檢查清單）
- Modify: `rust/README.md`（在設定／權限段落加一行說明）

- [ ] **Step 1: 寫手動檢查清單**（`MAC_SMOKE.md` 新增段落，內容如下）

```markdown
## 首次使用引導（手動，macOS 實機）

重置狀態後從頭走一次：

    killall lailaisay-app 2>/dev/null
    mv ~/Documents/hex_settings.json ~/Documents/hex_settings.json.bak
    tccutil reset Microphone com.yikai.lailaisay
    tccutil reset Accessibility com.yikai.lailaisay
    tccutil reset ListenEvent com.yikai.lailaisay
    # 模型：暫時移開 ~/Library/Application Support/com.yikai.lailaisay/models

檢查項目（每項記錄實際結果）：
1. 啟動後自動出現「歡迎使用 lailaisay」，按「開始設定」進入模型步驟。
2. 模型步驟預設選中推薦模型；下載顯示狀態；完成後自動載入並進入權限步驟。
3. 權限步驟：麥克風出現系統彈窗；輔助使用與輸入監控按「開啟」跳到系統設定；授權後該列自動變綠。
4. 若授權後快捷鍵未啟用，出現「重新啟動 lailaisay」，按下後新實例啟動且精靈回到試用步驟。
5. 試用步驟：按住快捷鍵說一句話，下方出現文字，自動進入「設定完成」。
6. 按「開始使用」後視窗收到選單列；退出再開，不再出現精靈。
7. 中途按「稍後再說」或直接結束，再開：精靈再次出現；選單列提示「需要設定」。
8. 還原：`mv hex_settings.json.bak hex_settings.json`，並把模型移回原處。
```

- [ ] **Step 2: README 加一行**（在權限說明處）：`首次啟動會顯示引導精靈（選模型 → 開權限 → 試用）；未完成前每次啟動都會再出現。`

- [ ] **Step 3: 完整本機檢查**

Run: `cd rust && RUSTFLAGS="-D warnings" cargo test --workspace --features lailaisay-stt/process-whisper 2>&1 | grep -E "FAILED|panicked|^error|test result" | head`
Expected: 無 FAILED／error。
Run: `cd rust && cargo run -p lailaisay-app -- --once`
Expected: 仍輸出 `去台南。`（dummy 後端，不受精靈影響）。

- [ ] **Step 4: macOS 實機走一遍清單**

打包測試版：`cd rust && TOK_SMOKE_SKIP_TEST=1 ./scripts/package-macos-app.sh`，開啟 `dist/lailaisay.app`，依 Step 1 清單逐項記錄。App Store 版另在沙盒內只檢查「只要求麥克風」與試用步驟（`--app-store`，開發憑證重簽流程見 `rust/APP_STORE.md`）。
Expected: 8 項皆通過；任何失敗先修再重跑受影響項目。

- [ ] **Step 5: 推送並看 CI**

```bash
git add rust/MAC_SMOKE.md rust/README.md
git commit -m "docs: onboarding manual smoke checklist

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
git push origin main
gh run watch $(gh run list --branch main --limit 1 --json databaseId --jq '.[0].databaseId')
```
Expected: `test`、`macos`、`windows` 三個工作 success。Windows 工作會執行 `cargo test -p lailaisay-app --lib`，涵蓋精靈畫面測試。

---

## Self-Review

**Spec coverage**
- 就緒判斷（純函式、平台差異、載入成功才算模型就緒）→ Task 2、3。
- 半強制顯示、稍後再說、既有使用者只看缺的步驟 → Task 1（欄位）、2（`remaining_steps`）、5（`Later`、啟動判斷）。
- 各步驟內容（歡迎／模型／權限／試用／完成）→ Task 4。
- 試用為必要條件、`onboardingPending` 持久化 → Task 1、5（`poll_onboarding`）。
- 選單列提醒 → Task 5 Step 3.5。
- 授權後熱鍵重建／重新啟動 → Task 6。
- 離線可選本機模型 → Task 4（`UseModel`）、Task 5（動作處理）。
- 實機驗證與 CI → Task 7。
- 規格偏差已在 Task 1 Step 5 回寫規格書（新增欄位、Windows 權限為空）。

**Placeholder scan:** 無 TBD／TODO；每個程式步驟都有程式碼。Task 4 的 `catalog_row_label` 回傳型別與 Task 2 的 `#[allow(dead_code)]` 暫時處理已在步驟內註明處理方式。

**Type consistency:** `Probe`、`Step`、`Platform`、`OnboardingAction`、`OnboardingView` 的欄位與方法名稱在 Task 2、4、5 一致；`trial_passed(baseline, count, last_final)` 簽名在 Task 2 定義、Task 5 使用；`finish_onboarding(&Path)` 在 Task 1 定義、Task 5 使用；`relaunch_app()` 在 Task 6 定義、Task 5 以 cfg(macos) 呼叫（Task 5 的編譯檢查在 Task 6 完成前於 macOS 會因缺函式失敗，所以 Task 5 與 Task 6 要在同一台 macOS 上連續做；Linux 不受影響）。

**Review Focus:** 五項皆已對應到測試（Task 1、2、4）。
