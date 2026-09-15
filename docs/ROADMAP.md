# folder-intelligence Roadmap

Status: Draft v0.1

The roadmap is deliberately staged. The project should validate the evidence and classification model before adding automatic actions or a large integration surface.

## Phase 0 — Stabilize the Evidence Engine

Goal: make the current Rust scanner trustworthy.

Status: **COMPLETE**

Tasks:

- verify and enforce `max_depth`
- enforce `max_total_files`
- enforce `max_total_dirs`
- define and test `max_files_per_dir`
- correct `total_size` semantics
- verify timeout behavior
- verify error reporting and partial-scan semantics
- normalize paths where required
- make `inspect` operate efficiently on a single directory
- improve Windows/Unicode path handling
- review symlink/reparse-point behavior
- reduce noisy or duplicate identifier output
- review unused or misleading dependencies
- expand unit and integration tests
- validate JSON Schema against emitted JSON

Completed:
- `partial_scan` bug fix for `max_total_files` limit
- All limits verified with tests
- 67+ tests passing, clippy clean, release build succeeds

No AI, MCP, SQLite, GUI, or filesystem mutation in this phase.

## Phase 1 — Evidence Quality

Goal: produce evidence that is genuinely useful to downstream AI.

Status: **COMPLETE**

Tasks:

- define exact semantics for every evidence field
- replace first-N-only filename sampling with bounded intelligent sampling
- preserve first/middle/last and unusual/high-information filenames where practical
- identify README/NFO/TXT/MD and other high-value files without semantic classification
- summarize large repetitive filename sequences
- add evidence-budget configuration
- add bounded text previews where justified
- define stable evidence identifiers/references
- design directory fingerprints
- test evidence quality on messy real-world directory trees

Completed:
- 5-phase tiered filename sampling (identifiers → notable → rare extensions → structural positions → fill by name)
- 12 internal unit tests for sampler quality
- Deterministic output verified across repeated scans
- No AI/SQLite/MCP/GUI/mutation introduced

## Phase 2 — Evidence Schema v2.0

Goal: strengthen the evidence contract with additional observational fields for downstream classification.

Status: **COMPLETE**

Added:
- `depth` — directory depth from scan root (0 = root)
- `is_empty` — explicit empty directory marker (derived: `file_count == 0 && directory_count == 0`)
- `dominant_extensions` — top extensions by frequency with counts and percentages
- `identifier_summary` — aggregate identifier counts (total + by type)
- `filename_sample` (renamed from `representative_filenames`) — clearer naming
- `syntactic_identifiers` (renamed from `potential_identifiers`) — aligns with "syntactic pattern" terminology
- `ScanMetadata` — scan-level provenance (schema_version, scan_batch_id, scan_started_at, root_path, limits, stats)
- JSONL header line with `ScanMetadata`
- Deterministic fixtures in `fixtures/` directory
- Updated JSON Schema (`schemas/directory-evidence.json` + `schemas/scan-metadata.json`)

Did NOT implement (deferred):
- `naming_pattern` (Sequential/Hashed/Mixed/Unknown) — too close to interpretation
- `identifier_density` — name/implementation mismatch; replaced with `identifier_summary`

Definition of Done:
- `cargo test` — PASS
- `cargo clippy -- -D warnings` — PASS
- `cargo build --release` — PASS
- JSON Schema validation — PASS
- Deterministic output — PASS

## Phase 3 — Classification Prototype

Goal: validate the central hypothesis before integrating AI into the Rust core.

Input:

```text
folders.jsonl
```

Run the same evidence through multiple classifiers, for example:

```text
Gemini
Gemma
Qwen
other compatible models
```

Do not modify the filesystem.

Test especially:

1. target directory vs populated existing category;
2. target directory vs several competing populated categories;
3. target directory vs empty category;
4. target directory where no existing category is suitable;
5. ambiguous cases where the correct answer should be ASK_USER or LEAVE_UNCLASSIFIED.

The benchmark should measure:

- correct destination
- correct CREATE_CATEGORY decisions
- correct refusal to classify
- false-positive moves
- false-negative moves
- confidence calibration
- evidence grounding
- consistency across models
- latency and cost

Success criterion:

The filesystem-precedent approach demonstrates useful classification quality on a representative benchmark.

## Phase 3 — Candidate Retrieval and Index

Goal: avoid sending the whole filesystem to an AI model.

Tasks:

- create a lightweight searchable evidence index
- retrieve candidate directories/categories using cheap structural signals
- compare fingerprints and filename patterns
- support populated-category precedent retrieval
- preserve links from candidates back to original evidence
- introduce SQLite only if it materially improves retrieval/persistence

The index remains a rebuildable cache. The filesystem remains the source of truth.

## Phase 4 — AI Interpretation Layer

Goal: formalize provider-neutral AI interpretation.

Tasks:

- define `Interpretation` schema
- define classification output schema
- define confidence and uncertainty representation
- define evidence references
- support multiple AI providers without coupling the core to one provider
- add progressive evidence requests
- store model/version/prompt metadata for reproducibility

Possible provider adapters:

- OpenAI-compatible APIs
- Anthropic
- Gemini
- AWS Bedrock
- OpenRouter
- Ollama/local models

The exact provider list is not a core architectural dependency.

## Phase 5 — Planning and Safe Filesystem Actions

Goal: turn classifications into safe, reviewable changes.

Pipeline:

```text
Classification
    -> Plan
    -> Validate
    -> Preview
    -> Apply
    -> Operation Log
    -> Undo
```

Tasks:

- define Plan schema
- define supported filesystem operations
- independently validate AI-generated paths
- detect destination conflicts
- detect stale plans
- support dry-run/preview
- record operation IDs and before/after state
- implement safe undo
- refuse unsafe rollback when filesystem state has changed

Automatic modification must remain opt-in.

## Phase 6 — Agent and Integration Interfaces

Goal: make the engine usable by external agents and applications.

Potential interfaces:

- richer CLI
- library/API
- MCP adapter
- agent-oriented search tools
- GUI adapter

MCP should be an adapter around the core rather than a core dependency.

## Phase 7 — Advanced Evidence

Only after the basic evidence/classification loop works reliably.

Potential additions:

- archive inspection
- media metadata
- full or partial hashing
- duplicate/near-duplicate detection
- OCR
- image/VLM evidence
- audio/video metadata and analysis
- embeddings for semantic retrieval

These should be conditional and budgeted rather than always-on.

## Phase 8 — Production Hardening

Potential work:

- large-filesystem benchmarks
- incremental rescanning
- change detection
- persistent cache invalidation
- crash recovery
- permission edge cases
- Windows reparse-point behavior
- long-path handling
- deterministic/reproducible output where appropriate
- security review of filesystem operations
- documentation for third-party integrations

## Current Priority

The immediate sequence is:

```text
1. Review current implementation          ✓ (done)
2. Stabilize scanner correctness          ✓ (Phase 0 done)
3. Validate evidence/schema semantics      ✓ (Phase 0 done)
4. Improve evidence sampling              ✓ (Phase 1 done)
5. Evidence schema v2.0 + fixtures       ✓ (Phase 2 done)
6. Generate real JSONL evidence           ✓ (Phase 2 done)
7. Build external classification benchmark (Phase 3 — pending)
8. Validate filesystem-precedent classification (Phase 3 — pending)
9. Only then implement AI/index/action layers (Phase 4+ — pending)
```

## Rules for AI Coding Agents

Before modifying the repository:

1. Read `docs/DESIGN.md`.
2. Read the relevant roadmap phase.
3. Inspect the existing implementation rather than assuming the README is accurate.
4. Do not implement future phases opportunistically.
5. Do not add AI, MCP, SQLite, GUI, or filesystem mutation during Phase 0 unless the task explicitly advances to that phase.
6. Add tests for behavioral changes.
7. Run the project's validation commands before committing.
8. Keep commits focused and explain architectural changes.

## Definition of Done for the Current Stage

The project is ready to move beyond Phase 2 only when the evidence schema is stable, deterministic output is verified, fixtures exist, and the emitted JSON conforms to the published schema. Phase 3 (classification benchmark) may proceed.
