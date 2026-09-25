# folder-intelligence 使用說明 (`docs/USAGE.md`)

## 1. 這是什麼？

`folder-intelligence` 是一個結合「檔案系統觀察引擎」與「AI 輔助檔案整理」的工具。

它可以分析指定資料夾中的檔案，透過規則或 LLM 判斷檔案應該如何分類，產生結構化的整理預覽，等待使用者確認後才真正執行檔案操作，最後透過 `ExecutionVerifier` 驗證整理結果。

基本執行流程：

```text
資料夾
  ↓
掃描檔案 (Scan)
  ↓
分析 / 分類 (Analysis / LLM Classifier)
  ↓
產生整理建議與提案 (OrganizationProposal)
  ↓
建立操作計畫與驗證 (OperationPlan & Validation)
  ↓
顯示預覽 (Preview)
  ↓
互動式 Refinement 或 使用者確認 ([r] Refine / [c] Confirm / [q] Quit)
  ↓
執行整理 (Execute)
  ↓
驗證結果 (ExecutionVerifier)
```

**核心安全原則**：
- **分析和預覽階段絕不自動移動檔案。**
- 只有使用者明確確認 (`[c]` / `y`) 後，才會執行實際的檔案操作。
- 每次操作後皆會透過 `ExecutionVerifier` 確保檔案來源已移除、目標完整且內容無誤。

---

## 2. Quick Start (快速開始)

1. 在專案根目錄執行編譯：
   ```powershell
   cargo build --release --features network
   ```
2. 對任意測試資料夾執行 `organize`：
   ```powershell
   .\target\release\fi.exe organize <目標資料夾>
   ```
3. 程式會列出 AI 組織提案與預覽操作，並顯示互動選單：
   ```text
   [r] Refine again
   [c] Confirm
   [q] Quit
   ```
   輸入 `c` 進行確認，並在確認提示中輸入 `y` 執行整理。

---

## 3. 基本使用與參數

### `fi organize <folder>`
主要入口，用於分析與整理指定資料夾。

### Zero-argument mode
如果不指定資料夾，直接執行：
```powershell
fi organize
```
程式會自動解析並使用預設的工作範圍（通常為執行目錄）。

### `--dry-run`
只進行分析、產生計畫與預覽，**完全不修改檔案系統**。適合用於測試與檢查整理邏輯：
```powershell
fi organize <folder> --dry-run
```

### `--yes`
跳過最終的「Proceed with execution? [y/N]」確認提示，直接執行操作（通常用於自動化腳本）：
```powershell
fi organize <folder> --yes
```

### `--instruction`
提供自定義的分類指導說明給 LLM：
```powershell
fi organize <folder> --instruction "請依照檔案年份分類"
```

### `--refine`
在啟動時直接帶入初次自然語言調整指令（維持向下相容）：
```powershell
fi organize <folder> --refine "把 AuthorA 改成 作者A"
```

---

## 4. 互動式 Refinement 流程

當執行 `fi organize <folder>` 後，顯示預覽與選單：

```text
[r] Refine again
[c] Confirm
[q] Quit
Enter choice:
```

- **`r` (Refine)**：
  輸入 `r` 後，程式會提示：
  ```text
  How would you like to change the proposal?
  >
  ```
  使用者可以輸入自然語言（例如：「將 misc.txt 移動到 AuthorB」或「把 AuthorA 改成 作者A」）。
  - 系統會透過 LLM 解析該指令並更新提案（`OrganizationProposal`）。
  - 若解析失敗或分類不存在，會回報錯誤並保留原提案（不破壞原有狀態）。
  - 支援連續多次進行 `r` 調整。

- **`c` (Confirm)**：
  確認目前提案與操作計畫無誤，離開互動迴圈，進入確認與執行階段。

- **`q` (Quit)**：
  取消並退出程式，**對檔案系統不做任何修改**。

---

## 5. Provider / LLM 設定

若要啟用 AI 分類與 Refinement，需在使用者目錄下的 `.folder-intelligence/config.toml` 設定 LLM Provider。

例如使用 OpenAI-compatible 服務（如 Gemini API）：
```toml
provider = "openai-compatible"
model = "gemini-3.5-flash-lite"
base_url = "https://generativelanguage.googleapis.com/v1beta/openai/"
api_key_env = "GEMINI_API_KEY"
timeout_seconds = 120
```

或使用本機 Ollama：
```toml
provider = "ollama"
model = "llama3"
endpoint = "http://localhost:11434"
timeout_seconds = 120
```

*注意：使用網路 LLM 功能時，編譯需加上 `--features network` 標籤。*

---

## 6. Safety & Verification (安全機制)

1. **嚴格隔離**：LLM 僅負責觀察與提供 `OrganizationProposal`（分類建議），不直接擁有檔案系統寫入權限。
2. **OperationPlan 與驗證**：所有提案會轉譯為明確的 `OperationPlan`，並經過 `Pipeline::validate()` 檢查衝突與越界路徑。
3. **ExecutionVerifier**：
   執行後，驗證器會自動檢查：
   - 來源檔案是否確實被移除
   - 目標檔案是否正確存在且內容完整
   - 目錄結構是否符合預期
   若驗證失敗會明確回報。

---

## 7. 其他常用指令

### 掃描資料夾（輸出 JSONL 證據）
```powershell
fi scan <folder> -o evidence.jsonl
```

### 檢視單一資料夾詳細結構
```powershell
fi inspect <folder>
```

### 輸出 JSON Schema
```powershell
fi schema > schema.json
```
