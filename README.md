# folder-intelligence

**Filesystem is the source of truth. Evidence is observation. Interpretation belongs to downstream AI or applications.**

A fast, low-resource filesystem evidence engine that observes and reports factual directory structure without semantic classification.

## Core Philosophy

- **Filesystem is the source of truth** — We observe what exists, not what we think should exist
- **Evidence is observation** — Raw, structured facts about directories and files
- **Interpretation belongs downstream** — No AI, no classification, no guessing. Just evidence.

## Features

- **JSONL output** — One evidence record per directory, streamable and parseable
- **Configurable limits** — Depth, file count, directory count, timeouts to prevent runaway scans
- **Windows compatible** — Handles Unicode paths, long paths, special filesystem structures
- **Low resource usage** — Streaming processing, bounded memory, parallel scanning
- **Extensible design** — Ready for SQLite indexing, AI interpretation, MCP server, action planning (not implemented)

## Installation

```bash
cargo install --git https://github.com/folder-intelligence/folder-intelligence
```

Or build from source:
```bash
git clone https://github.com/folder-intelligence/folder-intelligence
cd folder-intelligence
cargo build --release
```

## Usage

### Scan a directory

```bash
# Scan and output JSONL to stdout
fi scan "D:\未分類"

# Scan with custom limits
fi scan "/path/to/dir" --max-depth 10 --max-total-files 500000

# Save to file
fi scan "/path/to/dir" -o evidence.jsonl
```

### Inspect a single directory

```bash
fi inspect "/path/to/dir"
```

### Get JSON Schema

```bash
fi schema > directory-evidence.json
```

## Evidence Structure

Each line of output is a `DirectoryEvidence` object:

```json
{
  "path": "/absolute/path/to/dir",
  "name": "dir",
  "parent_path": "/absolute/path/to",
  "file_count": 42,
  "directory_count": 5,
  "total_size": 104857600,
  "extension_histogram": {
    "txt": 15,
    "pdf": 10,
    "jpg": 8,
    "mp3": 5,
    "(no extension)": 4
  },
  "child_directory_names": ["subdir1", "subdir2", "..."],
  "representative_filenames": ["file1.txt", "document.pdf", "..."],
  "notable_filenames": ["README.md", "LICENSE", "CHANGELOG.txt"],
  "potential_identifiers": [
    {
      "value": "978-0-306-40615-7",
      "source_filename": "book_978-0-306-40615-7.pdf",
      "identifier_type": "isbn"
    }
  ],
  "text_file_presence": {
    "has_readme": true,
    "has_nfo": false,
    "has_txt": true,
    "has_md": true,
    "has_license": true,
    "has_changelog": true,
    "text_files_found": ["README.md", "LICENSE", "CHANGELOG.txt"]
  },
  "partial_scan": false,
  "scanned_at": 1700000000,
  "scan_duration_ms": 42
}
```

### Identifier Types (Observational Only)

The following patterns are detected in filenames **without assigning semantic meaning**:

- `isbn` — ISBN-10/13
- `doi` — Digital Object Identifier
- `uuid` — UUID
- `semver` — Semantic version
- `hash` — MD5/SHA1/SHA256
- `date` — Various date formats
- `alphanumeric_code` — Patterns like SKU-001, ABC123
- `email` — Email addresses
- `url` — URLs

**Important**: These are purely syntactic pattern matches. An ISBN in a filename doesn't mean the folder "is a book". It means a filename contains an ISBN-like string.

## Limits (Preventing Runaway Scans)

| Limit | Default | Description |
|-------|---------|-------------|
| `max_depth` | 50 | Maximum directory depth |
| `max_files_per_dir` | 10,000 | Files to process per directory |
| `max_total_files` | 1,000,000 | Total files across scan |
| `max_total_dirs` | 100,000 | Total directories across scan |
| `max_representative_files` | 20 | Sample filenames per directory |
| `max_child_dirs` | 500 | Child directory names recorded |
| `timeout_seconds` | 3600 | Scan timeout (0 = none) |

All limits are configurable via CLI flags.

## Output Schema

The JSON Schema is available at `schemas/directory-evidence.json` and via `fi schema`.

Use it to validate evidence in downstream applications:

```bash
fi schema > schema.json
# Use with any JSON Schema validator
```

## Development

### Run tests

```bash
cargo test
```

### Lint

```bash
cargo clippy -- -D warnings
```

### Build release

```bash
cargo build --release
```

## Architecture

```
src/
├── main.rs       # Entry point
├── lib.rs        # Public API
├── cli.rs        # CLI commands (scan, inspect, schema)
├── evidence.rs   # Core data types + JSON Schema
└── scanner.rs    # Filesystem scanning + identifier extraction
```

### Designed for Future Extension

The codebase is structured to support (not yet implemented):
- **SQLite index** — Persistent evidence storage
- **AI interpretation** — LLM-based classification on evidence
- **Progressive evidence** — Incremental scanning, change detection
- **MCP server** — Model Context Protocol for AI agents
- **Action planner** — Filesystem operations based on evidence

## License

MIT OR Apache-2.0