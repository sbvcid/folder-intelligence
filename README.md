# folder-intelligence

**Filesystem is the source of truth. Evidence is observation. Interpretation belongs to downstream AI or applications.**

A fast, low-resource filesystem evidence engine and AI-powered folder organization tool.

## Core Features

- **Filesystem Observation** — Scan and inspect directory structures with robust JSONL output.
- **AI-Powered Organization (`organize`)** — Automatically classify and organize files based on structured recommendations and LLM providers (Ollama, OpenAI-compatible).
- **Interactive Refinement** — Refine organization proposals iteratively using natural language (`[r]` to refine, `[c]` to confirm, `[q]` to quit safely without modifications).
- **Safe Execution & Verification** — Every operation is planned, validated, confirmed, executed, and verified by `ExecutionVerifier` to guarantee file integrity.

---

## Quick Start

1. 確保已透過 `cargo build --release --features network` 編譯出 `target/release/fi.exe`。
2. 在目標資料夾執行：

```bash
fi organize .
```

程式會分析資料夾、顯示整理建議與預覽，並等待您的確認。

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
```bash
cargo test --features network
```

### Build release
```bash
cargo build --release --features network
```

## License

MIT OR Apache-2.0
