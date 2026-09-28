## Archived

This project is archived and is no longer actively developed.

The project originally explored building a standalone filesystem intelligence and organization engine. After evaluating the approach against current AI agent and MCP tooling, the standalone implementation was no longer necessary for its original goal.

The repository is preserved for historical reference and future experimentation.

# folder-intelligence

**Folder Intelligence 1.0 = File-level AI Organizer**
**掃描資料夾 → 分析檔案 → LLM 分類 → ClassificationResult → OperationPlan → Preview → Confirm → ExecutionVerifier → 實際執行**

**Filesystem is the source of truth. Evidence is observation. Interpretation belongs to downstream AI or applications.**

A fast, low-resource filesystem evidence engine and AI-powered folder organization tool.

## Core Features

- **Filesystem Observation** — Scan and inspect directory structures with robust JSONL output.
- **AI-Powered Organization (`organize`)** — File-level classification through an LLM, producing a
  `ClassificationResult` and an `OrganizationProposal` that is converted into an `OperationPlan`.
  Providers: OpenAI-compatible (e.g. Gemini 3.5 Flash Lite) and Ollama.
- **Interactive Refinement** — Refine organization proposals iteratively using natural language (`[r]` to refine, `[c]` to confirm, `[q]` to quit safely without modifications).
- **Safe Execution & Verification** — Every operation is planned, validated, confirmed, executed, and verified by `ExecutionVerifier` to guarantee file integrity.

### 1.0 Scope

Classification is **file-level**. The tool classifies individual files within a target folder; it does
not infer folder roles or perform hierarchical/recursive organization. Proposals that reference files
absent from the scan are rejected, and items marked `AskUser` or `LeaveUnclassified` never produce a
filesystem mutation.

---

## Quick Start

1. 確保已透過 `cargo build --release --features network` 編譯出 `target/release/fi.exe`。
2. 在目標資料夾執行：

```bash
fi organize .
```

程式會分析資料夾、顯示整理建議與預覽，並等待您的確認。

### 圖形介面直接啟動

若將 `fi.exe` 複製到目標資料夾後**直接雙擊**（不帶任何參數），程式會自動以 `fi.exe` 所在的資料夾
作為整理目標，並在結束後暫停於 `Press Enter to close this window...`，讓您得以閱讀完整輸出。
若在指令列帶參數執行，則不會暫停，適合腳本與 CI 使用。

---

## Usage Guide (`organize`)

### Basic Usage

```bash
# Organize current or default folder (zero-argument mode)
fi organize

# Organize a specific folder
fi organize "C:\Users\User\Downloads"

# Dry run (preview only, no filesystem changes)
fi organize C:\Downloads --dry-run

# Skip final confirmation prompt
fi organize C:\Downloads --yes
```

### Advanced Options

```bash
# Provide custom instructions to the classifier
fi organize C:\Downloads --instruction "Group documents by year"

# Apply an initial natural-language refinement immediately
fi organize C:\Downloads --refine "把 AuthorA 改成 作者A"
```

### Interactive Refinement Loop

When running `fi organize <folder>`, after previewing the recommendation and proposal, you will be prompted:

```text
[r] Refine again
[c] Confirm
[q] Quit
Enter choice: 
```

- **`r`**：輸入自然語言來修改分類提案 (例如：「將 misc.txt 移動到 AuthorB」)。可進行多次連續調整。
- **`c`**：確認並準備執行操作。
- **`q`**：退出程式，**不對檔案系統做任何修改**。

---

## LLM Provider Setup

To use AI classification, configure your provider in `~/.folder-intelligence/config.toml`:

```toml
provider = "openai-compatible"
model = "gemini-3.5-flash-lite"
base_url = "https://generativelanguage.googleapis.com/v1beta/openai/"
api_key_env = "GEMINI_API_KEY"
```

Or use Ollama locally:
```toml
provider = "ollama"
model = "llama3"
endpoint = "http://localhost:11434"
```

---

## Other Commands

### Scan a directory
```bash
fi scan "D:\未分類" -o evidence.jsonl
```

### Inspect a single directory
```bash
fi inspect "/path/to/dir"
```

### Get JSON Schema
```bash
fi schema > directory-evidence.json
```

---

## Documentation

For full details, please refer to [`docs/USAGE.md`](docs/USAGE.md).

## Development

### Run tests

Integration test targets are self-contained and pass as-is:

```bash
cargo test --all-features --test integration_tests
cargo test --all-features --test unit_tests
```

> **Known issue in 1.0:** four `cli` tests (`test_organize_empty_folder_friendly_error`,
> `test_organize_with_yes_executes_and_verifies`, `test_organize_non_directory_scope_error`,
> `test_organize_no_yes_flag_cancellation`) read the real user config at
> `~/.folder-intelligence/config.toml`. When the `network` feature is enabled they attempt live
> LLM calls, which makes the combined `cargo test --all-features` lib target hang or exhaust
> memory. Each of these tests passes when run individually. The fix is to isolate the config
> path per test; this is tracked for a post-1.0 release and was deliberately not addressed here.

### Build release

```bash
cargo build --release --features network
```

Release binaries are built with `strip = false` (symbols retained) because stripped builds were
falsely flagged by Microsoft Defender as `Trojan:Win32/Bearfoos.A!ml`.

## License

MIT OR Apache-2.0
