# folder-intelligence

**Filesystem is the source of truth. Evidence is observation. Interpretation belongs to downstream AI or applications.**

A fast, low-resource filesystem evidence engine that observes and reports factual directory structure without semantic classification.

## Core Philosophy

- **Filesystem is the source of truth** — We observe what exists, not what we think should exist
- **Evidence is observation** — Raw, structured facts about directories and files
- **Interpretation belongs downstream** — No AI, no classification, no guessing. Just evidence.

## Features

- **JSONL output** — One evidence record per directory, streamable and parseable
- **Scan-level metadata** — Batch ID, schema version, timestamps, and scan stats
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

# Quiet mode (no stderr status messages)
fi scan "/path/to/dir" --quiet
```

### Inspect a single directory

```bash
fi inspect "/path/to/dir"
```

### Get JSON Schema

```bash
fi schema > directory-evidence.json
```

## JSONL Output Format

Output is JSONL (one JSON object per line). The first line is a `ScanMetadata` header, followed by one `DirectoryEvidence` record per directory:

```text
Line 1: ScanMetadata (with _type: "scan_metadata")
Line 2..N: DirectoryEvidence (one per directory)
```

### ScanMetadata (header line)

```json
{
  "_type": "scan_metadata",
  "schema_version": "2.0.0",
  "scan_batch_id": "18d564caaed7d98c",
  "scan_started_at": 1700000000,
  "root_path": "/absolute/path/to/root",
  "limits": { "max_depth": 50, ... },
  "stats": { "directories_scanned": 5, ... }
}
```

### DirectoryEvidence

```json
{
  "path": "/absolute/path/to/dir",
  "name": "dir",
  "parent_path": "/absolute/path/to",
  "depth": 1,
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
  "dominant_extensions": [
    { "extension": "txt", "count": 15, "percentage": 35.71 }
  ],
  "identifier_summary": {
    "total": 2,
    "by_type": { "isbn": 1, "uuid": 1 }
  },
  "child_directory_names": ["subdir1", "subdir2", "..."],
  "filename_sample": ["file1.txt", "document.pdf", "..."],
  "notable_filenames": ["README.md", "LICENSE", "CHANGELOG.txt"],
  "syntactic_identifiers": [
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
  "is_empty": false,
  "partial_scan": false,
  "scanned_at": 1700000000,
  "scan_duration_ms": 42,
  "schema_version": "2.0.0"
}
```

### Field Reference

| Field | Description |
|-------|-------------|
| `path` | Absolute path of the directory |
| `name` | Directory name (last path component) |
| `parent_path` | Parent directory path (null for root) |
| `depth` | Depth from scan root (0 = root) |
| `file_count` | Number of files directly in this directory |
| `directory_count` | Number of subdirectories directly in this directory |
| `total_size` | Total size of direct files in bytes (not recursive) |
| `extension_histogram` | Frequency map of file extensions (lowercase, no dot) |
| `dominant_extensions` | Top extensions by frequency with counts and percentages |
| `identifier_summary` | Aggregate count of syntactic identifiers (total + by type) |
| `child_directory_names` | Immediate child directory names (bounded by `max_child_dirs`) |
| `filename_sample` | Bounded sample of filenames using tiered selection |
| `notable_filenames` | Common/special filenames observed (README, LICENSE, etc.) |
| `syntactic_identifiers` | Syntactic identifiers extracted from filenames (no semantic meaning) |
| `text_file_presence` | Boolean flags for common text files (README, NFO, TXT, MD, LICENSE, CHANGELOG) |
| `is_empty` | `true` when `file_count == 0 AND directory_count == 0` |
| `partial_scan` | `true` when scan was truncated by limits |
| `scanned_at` | Scan timestamp (Unix epoch seconds) |
| `scan_duration_ms` | Duration to scan this directory in milliseconds |
| `schema_version` | Schema version identifier (e.g. `"2.0.0"`) |

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
- `custom` — Custom pattern (string value provided)

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
