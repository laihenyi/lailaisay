# lailaisay 隱私權政策 / Privacy Policy

生效日期 / Effective date: 2026-09-21

lailaisay 是一款在你的電腦上執行的語音輸入工具。本政策說明 lailaisay 如何處理你的資料。
lailaisay is a voice dictation tool that runs on your computer. This policy explains how lailaisay handles your data.

## 1. 我們不收集資料 / We do not collect data

lailaisay 沒有帳號、沒有伺服器、沒有分析或追蹤工具。開發者不會收到你的錄音、辨識文字、設定或任何使用資料。
lailaisay has no accounts, no servers, and no analytics or tracking. The developer never receives your recordings, transcribed text, settings, or any usage data.

## 2. 麥克風與語音辨識 / Microphone and speech recognition

- 只有在你按住快捷鍵時才會錄音，放開即停止。
  Audio is recorded only while you hold the hotkey and stops when you release it.
- 語音辨識由內建的 whisper.cpp 在本機完成。錄音不會離開你的電腦，辨識後即丟棄，不會存檔。
  Speech recognition runs locally with the bundled whisper.cpp. Audio never leaves your computer; it is discarded after recognition and is not saved.
- 辨識結果只會寫入系統剪貼簿或貼入你正在使用的 App。
  The recognized text is only written to the system clipboard or pasted into the app you are using.

## 3. 本機儲存的資料 / Data stored on your device

設定、自訂辭典與校正紀錄只儲存在你電腦的使用者目錄（macOS 為 App 的沙盒容器或 `~/Documents`，Windows 為 `%APPDATA%`）。你可以隨時刪除這些檔案。
Settings, custom dictionaries, and correction history are stored only in your user directory (the app's sandbox container or `~/Documents` on macOS, `%APPDATA%` on Windows). You can delete these files at any time.

## 4. 網路連線 / Network connections

lailaisay 只在下列情況使用網路 / lailaisay uses the network only in these cases:

- **下載語音模型 / Model downloads**：首次使用時，從 Hugging Face（`huggingface.co`）下載 Whisper 模型檔。此連線只傳送一般的下載請求，不含你的個人資料。
  On first use, Whisper model files are downloaded from Hugging Face (`huggingface.co`). This request contains no personal data.
- **可選的 AI 潤稿 / Optional AI polish**：此功能預設關閉。若你自行啟用並提供 API 金鑰，辨識出的**文字**（不含錄音）會傳送給你選擇的服務商處理：
  This feature is off by default. If you enable it and supply your own API key, the recognized **text** (never the audio) is sent to the provider you choose:
  - Groq（`api.groq.com`）— 適用 [Groq 隱私權政策](https://groq.com/privacy-policy/) / subject to Groq's privacy policy
  - Google Gemini（`generativelanguage.googleapis.com`）— 適用 [Google 隱私權政策](https://policies.google.com/privacy) / subject to Google's privacy policy
  - Ollama — 連線到你本機或自行指定的伺服器，資料不經過第三方 / connects to your own local or self-hosted server; no third party is involved

API 金鑰只儲存在本機設定檔或環境變數中，不會傳送給開發者。
API keys are stored only in your local settings file or environment variables and are never sent to the developer.

## 5. 系統權限 / System permissions

- **麥克風 / Microphone**：錄音用 / used for recording.
- Mac App Store 版本只需要麥克風權限。直接下載版另可請求「輔助使用」與「自動化」權限，僅用來將文字貼入前景 App，不會讀取或記錄你的其他按鍵或畫面。
  The Mac App Store version needs only the microphone. The direct-download version may also request Accessibility and Automation, used solely to paste text into the frontmost app; it does not read or log your other keystrokes or screen.

## 6. 兒童 / Children

lailaisay 不針對 13 歲以下兒童設計，也不會蓄意收集任何人的個人資料。
lailaisay is not directed at children under 13 and does not knowingly collect personal data from anyone.

## 7. 政策變更 / Changes

政策更新會發佈在本頁面，並更新上方的生效日期。
Updates will be posted on this page with a new effective date above.

## 8. 聯絡方式 / Contact

如有任何疑問，請透過 GitHub 專案頁面聯絡我們：https://github.com/laihenyi/lailaisay
For questions, contact us via the GitHub project page: https://github.com/laihenyi/lailaisay
