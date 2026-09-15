# Phase 2 — Evidence Schema v2.0

## Goal

Strengthen the evidence contract so downstream classification (Phase 3) can operate on **deterministic, well-defined observations**.

No AI. No SQLite. No MCP. No GUI. No filesystem mutation. No file-content reading.

## Scope: What IS implemented

### Directory-level (in `DirectoryEvidence`)

| Field | Type | Rationale |
|-------|------|-----------|
| `depth` | `usize` | Depth from scan root (0 = root). Enables tree-aware precedent matching. |
| `is_empty` | `bool` | Explicit empty-directory marker. Derived: `file_count == 0 && directory_count == 0`. |
| `dominant_extensions` | `Vec<DominantExtension>` | Top extensions with counts and percentages. Convenience summary over `extension_histogram`. |
| `identifier_summary` | `IdentifierSummary` | Count of identifiers total + counts per `IdentifierType`. Replaces per-identifier noise with aggregate signal. |

### Renames (semantic clarity)

| Old | New | Reason |
|-----|-----|--------|
| `representative_filenames` | `filename_sample` | It is a sample, not a semantic "representative" selection |
| `potential_identifiers` | `syntactic_identifiers` | Aligns with DESIGN.md terminology — syntactic pattern, not semantic meaning |

### Scan-level provenance metadata (in `ScanMetadata`)

| Field | Type | Rationale |
|-------|------|-----------|
| `schema_version` | `String` | e.g. `"2.0.0"` — enables result provenance and benchmark reproducibility |
| `scan_batch_id` | `String` | Unique identifier for this scan batch (UUID v4) |
| `scan_started_at` | `u64` | Unix epoch seconds — overall scan start, not per-directory |
| `root_path` | `PathBuf` | Scan root |
| `limits` | `ScanLimits` | Applied limits (echoed for reproducibility) |
| `stats` | `ScanStats` | Aggregated statistics |

### Output format

JSONL with scan-level metadata as a header line:

```text
Line 1: ScanMetadata (JSON object with _type: "scan_metadata")
Line 2..N: DirectoryEvidence (JSON objects)
```

Each `DirectoryEvidence` line also includes `schema_version` for self-containment.

## Scope: What is NOT implemented

| Feature | Reason |
|---------|--------|
| `naming_pattern` (Sequential/Hashed/Mixed/Unknown) | Too close to interpretation; `Sequential = media series` is semantic classification |
| `identifier_density` | Name implies ratio (identifiers/files) but proposal was count-only; renamed to `identifier_summary` instead |
| `text_files_found` removal | Keeping existing `TextFilePresence` structure unchanged for Phase 2 to minimize churn |
| Batch envelope as separate object (instead of header line) | JSONL header line is simpler and more compatible with streaming tools |
| Complex filename pattern analysis | Deferred to Phase 3 — let AI interpret filename patterns from `filename_sample` + `extension_histogram` |

## Design Rule Adherence

> "Filesystem is the source of truth. Evidence is observation. Interpretation belongs downstream."

All Phase 2 additions are **observations**:
- `depth`: observed tree position
- `is_empty`: observed counts
- `dominant_extensions`: summarized from observed histogram
- `identifier_summary`: counted syntactic patterns
- Provenance metadata: recorded scan parameters, not interpretation

## Definition of Done

- `cargo test` — PASS
- `cargo clippy -- -D warnings` — PASS
- `cargo build --release` — PASS
- JSON Schema validation — PASS
- Deterministic output (same input → identical evidence) — PASS
- No AI/SQLite/MCP/GUI/mutation/content-reading introduced
