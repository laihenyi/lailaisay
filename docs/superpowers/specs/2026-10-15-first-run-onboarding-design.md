# 首次使用引導（Onboarding）設計

日期：2026-10-15　狀態：待使用者審閱

## 目標
新使用者安裝後，被逐步帶到「能實際聽寫」的狀態，不會卡在不明原因的空結果。兩個必經步驟：選擇並下載語音模型、開啟必要的系統權限。
成功標準：完成引導後，按住熱鍵說一句話能成功貼出文字。

## 現況（已讀程式確認）
- 首次啟動只開啟設定視窗；`hasCompletedOnboarding` 在啟動當下即寫入（`host.rs` `complete_onboarding`），關掉視窗後不再提醒。
- 權限探測已存在：`lailaisay_input::{microphone,accessibility,input_monitoring,automation}_grant()`；設定頁「系統權限」每 0.5 秒重新探測。App Store 版只需麥克風。
- 模型選擇與下載已存在（設定「語音模型」頁，`models.rs` CATALOG，Qwen 僅 macOS 可見）。未選模型會退成 dummy，使用者得到空結果且看不到原因。
- Windows 無 TCC 權限，只有麥克風與熱鍵。

## 決定
- 形式：設定視窗內的全視窗多步驟精靈（方案 A）。不開第二個視窗，不新增依賴。
- 強制程度：半強制。未就緒時每次啟動都顯示，每步可「稍後再說」（僅本次執行隱藏）。
- 不新增設定欄位；`hasCompletedOnboarding` 改為「全部就緒時」才寫入。舊 camelCase 設定檔相容。

## 元件
1. `onboarding.rs`（`lailaisay-app`）
   - `Readiness`：輸入平台／版本（Developer ID、App Store、Windows）、模型載入結果、各權限 `GrantStatus`；輸出缺少的步驟清單（純函式，可單元測試）。
   - 模型就緒＝實際載入成功；dummy 或載入失敗視為未就緒。
   - 必要權限：Developer ID 版＝麥克風＋輔助使用＋輸入監控；App Store 版與 Windows＝麥克風。
2. 精靈畫面（`settings_ui.rs` 新模式，未就緒時取代窗格區）
   - 步驟：歡迎 → 語音模型 → 系統權限 → 試用 → 完成；已就緒的步驟自動略過。
   - 模型：推薦卡片（Mac 預設 Qwen3-ASR 1.7B Q4 約 1.3 GB；Windows 預設 large-v3-turbo 約 1.6 GB；備選 Whisper small 約 466 MB）；下載顯示進度，載入成功才算完成；離線時可改選本機已有模型。重用 `start_catalog_download` 與 `poll_download`。
   - 權限：每項一列（狀態＋「開啟系統設定」），每 0.5 秒重新探測；麥克風走系統彈窗。重用 `perm_row` 邏輯。
   - 試用：請使用者按住熱鍵說一句話並顯示結果。此步可跳過，不是完成的必要條件（假設，使用者未明確回答）。
3. 啟動流程（`host.rs`）
   - 每次啟動計算 `Readiness`；未就緒則顯示設定視窗並進入精靈。
   - 取消啟動時立即 `complete_onboarding`；改為精靈完成（全部就緒）時寫入。
4. 選單列：未就緒時狀態列顯示「需要設定，點此繼續」，點擊重新打開精靈。

## 待實測的風險
- 輔助使用／輸入監控授權後，CGEvent tap 需重建才生效（現有訊息為 Quit and relaunch）。目標：授權後自動重建 tap；若做不到，提供「重新啟動 lailaisay」按鈕。實作時在 macOS 實機驗證。
- TCC 流程只能 macOS 實機驗證（`tccutil reset` 重置後走完全流程）；Windows 僅靠 CI。

## 非目標
- 不改 entitlements、不新增權限，App Store 審核不受影響。
- 不鎖死其他功能（非完全強制）。
- 商店截圖更新另案處理。

## 測試
- 單元：`Readiness` 在各平台／版本組合的缺漏清單、步驟順序、略過已就緒步驟。
- 畫面：沿用現有 egui 繪製測試，確認各步驟標題不被標題列遮住。
- 啟動：`runtime_opts` 測試更新為新語意（未就緒顯示、就緒且 minimize 設定開啟則留在選單列）。
- 實機：macOS 重置 TCC 後完整走一遍並記錄結果。
