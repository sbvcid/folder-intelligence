# folder-intelligence 使用說明

## 1. 這是什麼？

`folder-intelligence` 是一個資料夾整理工具。

它可以分析指定資料夾中的檔案，利用規則或 LLM 判斷檔案應該如何分類，產生整理預覽，等待使用者確認後才真正移動檔案，最後再驗證整理結果。

基本流程：

```text
資料夾
  ↓
掃描檔案
  ↓
分析 / 分類
  ↓
產生整理建議
  ↓
顯示預覽
  ↓
使用者確認
  ↓
執行整理
  ↓
驗證結果
```

重要的是：

**分析和預覽階段不會自動移動檔案。**

只有使用者確認後，才會執行實際的檔案操作。

---

## 2. 最簡單的使用方式

將：

```text
folder-intelligence.exe
```

放到想要整理的資料夾裡。

例如：

```text
Downloads/
├── folder-intelligence.exe
├── 2026-tax.pdf
├── invoice-123.pdf
├── vacation.jpg
├── project-alpha.zip
├── notes.txt
└── random.dat
```

直接雙擊：

```text
folder-intelligence.exe
```

程式會把 **exe 所在的資料夾** 當成預設分析範圍。

因此在上面的例子中，它會分析：

```text
Downloads/
```

而不是 Windows 目前的工作目錄。

---

## 3. 第一次使用建議

第一次使用時，建議先建立一個測試資料夾。

例如：

```text
test-folder/
├── 2026-tax.pdf
├── invoice-123.pdf
├── vacation.jpg
├── project-alpha.zip
├── notes.txt
└── random.dat
```

把：

```text
folder-intelligence.exe
```

放進去。

然後雙擊執行。

這樣可以先確認：

1. LLM 是否正常連線
2. 檔案是否被正確分析
3. 分類結果是否合理
4. 預覽是否符合預期
5. 確認後是否真的完成整理
6. 最後驗證是否成功

---

## 4. 命令列使用方式

除了直接雙擊，也可以使用命令列。

基本形式：

```text
folder-intelligence.exe organize <資料夾>
```

例如：

```text
folder-intelligence.exe organize C:\Users\User\Downloads
```

程式會依序：

```text
掃描
→ 分類
→ 建議
→ 建立操作計畫
→ 驗證計畫
→ 顯示預覽
→ 等待確認
→ 執行
→ 驗證
```

---

## 5. 不指定資料夾

直接執行：

```text
folder-intelligence.exe
```

等同於使用：

```text
folder-intelligence.exe organize <exe所在資料夾>
```

因此最簡單的使用方式就是：

```text
把 exe 放進資料夾
↓
雙擊 exe
↓
查看整理建議
↓
確認
```

---

## 6. 先看結果，不執行整理

可以使用：

```text
folder-intelligence.exe organize <資料夾> --dry-run
```

例如：

```text
folder-intelligence.exe organize C:\Users\User\Downloads --dry-run
```

`--dry-run` 只會分析並產生預覽。

不會實際移動檔案。

適合第一次測試或想先檢查 LLM 判斷時使用。

---

## 7. 顯示整理預覽

程式會把建議的操作整理成類似：

```text
Documents
  2026-tax.pdf
  invoice-123.pdf
  notes.txt

Images
  vacation.jpg

Projects
  project-alpha.zip

Unclassified
  random.dat
```

同時顯示需要執行的操作數量。

例如：

```text
3 files → Documents
1 file  → Images
1 file  → Projects
1 file  → Unclassified
```

在這個階段檔案尚未被移動。

---

## 8. 使用者確認

預覽完成後，程式會等待使用者確認。

如果取消：

```text
Cancel
```

則不執行檔案操作。

原始資料夾保持不變。

如果確認：

```text
Yes
```

才會開始執行整理。

---

## 9. 自動確認

如果希望程式不詢問確認，可以使用：

```text
folder-intelligence.exe organize <資料夾> --yes
```

例如：

```text
folder-intelligence.exe organize C:\Users\User\Downloads --yes
```

這會直接執行整理。

因此：

**第一次使用不要使用 `--yes`。**

先確認預覽結果符合預期，再考慮使用。

---

## 10. LLM 模式

folder-intelligence 支援 LLM 分類。

目前架構將 LLM 與檔案整理核心分離，因此可以使用不同的 LLM provider。

目前支援的主要模式包括：

```text
Ollama
OpenAI-compatible API
```

架構為：

```text
folder-intelligence
        ↓
    LLM Provider
       ↙    ↘
   Ollama   API
        ↓
   Classification
        ↓
   Recommendation
        ↓
      Plan
        ↓
     Execute
        ↓
     Verify
```

LLM 只負責：

**判斷檔案應該屬於哪一個分類。**

LLM 不直接操作檔案系統。

實際檔案移動仍由 folder-intelligence 自己的 Pipeline / Executor 執行。

---

## 11. Ollama

如果使用 Ollama，先確認 Ollama 正常運作。

例如本機：

```text
http://localhost:11434
```

並確認需要使用的模型已經下載。

例如：

```text
ollama list
```

確認模型存在。

之後在 folder-intelligence 的 LLM 設定中指定：

```text
provider = "ollama"
```

以及對應的：

```text
model
endpoint
```

具體設定以目前專案中的 LLM 設定檔為準。

---

## 12. OpenAI-compatible API

如果使用其他提供 OpenAI-compatible API 的服務，可以指定：

```text
provider = "openai-compatible"
```

然後設定：

```text
endpoint
model
api key
```

這種方式可以讓 folder-intelligence 使用不同的 API 服務，而不需要修改核心分類與整理程式。

---

## 13. LLM 分類的限制

LLM 不可以任意建立檔案分類。

它只能在程式提供的候選分類中選擇。

例如程式提供：

```text
Documents
Images
Projects
Media
```

LLM 只能回傳這些分類。

不能自行產生：

```text
ImportantThings
MySpecialFolder
RandomCategory
```

如果 LLM 回傳不存在的分類，分類結果會被拒絕。

同樣地，LLM 不能指定：

```text
C:\Windows\...
```

或：

```text
..\other-folder
```

等越界路徑。

---

## 14. 不確定的檔案

如果 LLM 無法可靠判斷檔案分類，可以讓檔案維持：

```text
Unclassified
```

這比強行猜測安全。

例如：

```text
random.dat
unknown.bin
```

可能會維持未分類狀態。

未分類檔案不會因為「整理」而被任意移動。

---

## 15. 整理完成後的驗證

執行完成後，程式會進行 Execution Verification。

驗證不是只看：

```text
move operation = success
```

而是會再次檢查實際檔案系統狀態。

例如：

```text
原始檔案是否已不存在於來源位置
目標檔案是否存在
目標檔案內容是否正確
建立的資料夾是否存在
應該刪除的項目是否真的不存在
不應該改變的項目是否保持不變
```

因此最後結果會區分：

```text
Execution succeeded
Verification passed
```

以及：

```text
Execution succeeded
Verification failed
```

後者表示操作本身完成，但最終檔案系統狀態沒有通過驗證。

---

## 16. 建議的第一次測試

建議使用以下測試資料：

```text
test-folder/
├── 2026-tax.pdf
├── invoice-123.pdf
├── vacation.jpg
├── project-alpha.zip
├── notes.txt
└── random.dat
```

執行：

```text
folder-intelligence.exe
```

先觀察預覽。

理想結果可能類似：

```text
Documents
  2026-tax.pdf
  invoice-123.pdf
  notes.txt

Images
  vacation.jpg

Projects
  project-alpha.zip

Unclassified
  random.dat
```

不要要求分類一定完全符合上述結果。

真正要確認的是：

```text
LLM
 ↓
Classification
 ↓
Recommendation
 ↓
OperationPlan
 ↓
Execution
 ↓
Verification
```

這整條鏈是否一致。

---

## 17. 如果只是想測試，不想修改檔案

使用：

```text
folder-intelligence.exe organize <資料夾> --dry-run
```

這是最安全的測試方式。

---

## 18. 如果整理結果不對

不要直接使用：

```text
--yes
```

先取消操作。

然後檢查：

1. LLM 使用的模型
2. LLM 分類結果
3. 預覽中的分類
4. 是否有檔案被標記為 Unclassified

目前設計的原則是：

**不確定就不要動。**

---

## 19. 建議的使用流程

一般使用者：

```text
1. 將 folder-intelligence.exe 放進要整理的資料夾
2. 雙擊 folder-intelligence.exe
3. 程式會分析資料夾內容並產生整理建議
4. 查看整理預覽
5. 確認後才會真正移動檔案
6. 整理完成後程式會自動驗證結果
```

進階使用者：

```text
folder-intelligence.exe organize <資料夾> --dry-run
```

確認結果後：

```text
folder-intelligence.exe organize <資料夾> --yes
```

---

## 20. 安全原則

folder-intelligence 的核心原則是：

```text
分析 ≠ 執行
```

LLM：

```text
負責理解與分類
```

Pipeline：

```text
負責建立與驗證操作計畫
```

Executor：

```text
負責實際檔案操作
```

Verification：

```text
負責確認實際結果
```

因此 LLM 不直接取得檔案系統操作權限。

完整流程為：

```text
Filesystem
    ↓
Observation
    ↓
LLM Classification
    ↓
ClassificationResult
    ↓
Recommendation
    ↓
OperationPlan
    ↓
Validation
    ↓
User Confirmation
    ↓
Executor
    ↓
Real Filesystem
    ↓
Execution Verification
```

---

## 21. 最簡短版本

如果只是要告訴一般使用者怎麼用：

```text
1. 把 folder-intelligence.exe 放進要整理的資料夾。

2. 雙擊 folder-intelligence.exe。

3. 程式會分析資料夾內容並產生整理建議。

4. 查看預覽。

5. 確認後才會真正移動檔案。

6. 整理完成後程式會自動驗證結果。

如果只想預覽而不修改檔案：

folder-intelligence.exe organize <資料夾> --dry-run
```

---

## 22. 注意事項

第一次使用時，建議先在測試資料夾中執行。

不要直接對包含重要原始資料的資料夾使用：

```text
--yes
```

先確認分類與預覽符合預期，再執行正式整理。
