# Phase 3 — Evidence-Driven Classification

## Status

Design complete. Ready for implementation.

## Goal

Design an evidence-driven filesystem classification layer on top of the existing deterministic scanner (Phase 0/1/2 complete). The classifier compares a target directory against existing candidate directories using filesystem precedent, producing structured classification decisions with evidence grounding.

## Core Principle

> "Filesystem is the source of truth. Evidence is observation. Interpretation belongs downstream. Actions require validation."

The classifier must **not** modify the filesystem. It must **not** make semantic assumptions about directory names. It must produce evidence-grounded decisions that a separate planning/validation layer consumes.

---

## 1. Classification Input Model

### `ClassificationInput`

```rust
pub struct ClassificationInput {
    /// Evidence for the target/unclassified directory
    pub target: DirectoryEvidence,
    /// Candidate directories with their evidence
    pub candidates: Vec<CandidateEvidence>,
    /// Scan provenance metadata
    pub metadata: ScanMetadata,
}
```

### `CandidateEvidence`

```rust
pub struct CandidateEvidence {
    /// Full DirectoryEvidence for the candidate category
    pub evidence: DirectoryEvidence,
    /// Representative evidence from the candidate's children
    /// (sample of child directory evidence, bounded for context window)
    pub child_evidence: Vec<DirectoryEvidence>,
    /// Category path (absolute path of the candidate directory)
    pub category_path: PathBuf,
}
```

### Key Design Decisions

- **Target**: Single `DirectoryEvidence` — the unclassified directory to classify
- **Candidates**: Existing directories on the filesystem that could serve as classification precedents
- **Child evidence**: A bounded sample of child directory evidence from each candidate (max `SAMPLE_CHILD_DIRS = 10`), providing structural context without flooding the model
- **Metadata**: Reuse `ScanMetadata` from Phase 2 for provenance

---

## 2. Category vs. Precedent (Architectural Clarification)

### Terminology

```text
Filesystem
├── Audio/                  ← Category (classification bucket)
│   ├── J-Pop/              ← Candidate (precedent directory under category)
│   │   ├── song001.mp3
│   │   └── song002.mp3
│   └── Classical/          ← Candidate (precedent directory under category)
│       ├── piece01.flac
│       └── piece02.flac
└── Unclassified/
    └── target_folder/      ← Target (to classify)
```

**Category** = a directory that represents a classification bucket. Its direct children are candidates.

**Candidate** = a non-empty child directory under a category root. Each candidate contributes one precedent example to that category's evidence profile.

### Classification Flow

1. The user runs: `fi classify <target> --category-root <root>`
2. `target` is scanned → `target` DirEvidence
3. `category-root` children are scanned → each child becomes a `CandidateEvidence`
4. For each candidate:
   - Use the child's `DirectoryEvidence` as primary evidence
   - Sample up to `SAMPLE_CHILD_DIRS` child directories' evidence as precedent examples
5. `Target` is compared against each candidate's evidence + precedent profile
6. Decision is made based on similarity scores

### Category-Level Evidence (Future, NOT in MVP)

A category's full evidence = aggregate over all its candidate children. This enables:
- "audio/RJ01547914" is a candidate under category "audio"
- Category "audio" evidence = aggregate of all audio category children

In Phase 3 MVP, each candidate directory is evaluated individually. Future Phase 3.x can add category-level aggregation.

### How empty directories are treated

- **Empty candidate directories are excluded** — they have no evidence of actual category content
- An empty directory is only a candidate if its **path** or **parent context** provides evidence (deferred to AI judgment)
- Empty directories are explicitly excluded from `candidates` but may be mentioned in `uncertainty`/`alternatives`
- A `partial_scan` directory should be treated as incomplete evidence — its score should be penalized or flagged

### How candidate evidence is bounded

- `SAMPLE_CHILD_DIRS = 10` — maximum child directory evidence sampled per candidate
- `SAMPLE_FILES_PER_DIR = 20` (from `max_representative_files`) — already bounded by scanner
- `MAX_CANDIDATES = 20` — maximum candidate directories considered in a single classification
- Candidates are ranked by heuristic pre-filter (extension overlap, structural similarity) before being returned, keeping output within budget

---

## 3. Comparison/Evidence Model

The classification layer computes the following **deterministic comparisons** (not interpretation — these are raw similarity signals):

### Extension Similarity
- Jaccard similarity on extension sets between target and candidate
- Dominant extension overlap (do they share the same top extension?)
- Extension distribution distance (cosine or Manhattan on normalized histograms)

### Structural Similarity
- File count ratio (target files vs candidate files)
- Directory count ratio
- Depth context comparison (target depth vs candidate depth)
- `is_empty` status

### Filename Pattern Similarity
- Overlap in `filename_sample` patterns (prefixes, suffixes, sequence markers)
- Notable filenames commonality (README, LICENSE presence)

### Identifier Similarity
- Identifier type overlap (`identifier_summary.by_type`)
- Identifier density comparison (total identifiers per file)
- Same identifier values appearing in both (e.g., same ISBN family patterns)

### Aggregate Precedent Strength
- Number of child directories in candidate exhibiting similar patterns
- Whether the candidate is a populated precedent (many files, well-established pattern)

### Partial Scan Handling
- If `target.partial_scan` is true, the result must include a warning in `uncertainty`
- If `candidate.evidence.partial_scan` is true, the candidate's score should be penalized

### These are signals, not decisions
The classifier outputs these comparisons as evidence in `supporting_evidence`. The decision is derived from the aggregate score and competitive landscape.

---

## 4. Classification Result Schema

### `ClassificationResult`

```json
{
  "target_path": "/unclassified/RJ99999999",
  "decision": "MOVE_EXISTING",
  "selected_candidate": {
    "path": "/Library/Audio/J-Pop",
    "category_path": "/Library/Audio/J-Pop"
  },
  "confidence": 0.94,
  "confidence_band": "high",
  "candidates_considered": 3,
  "supporting_evidence": [
    {
      "type": "extension_similarity",
      "description": "Target is 96% mp3; candidate is 92% mp3",
      "score": 0.96,
      "details": {
        "target_dominant": "mp3(96%)",
        "candidate_dominant": "mp3(92%)",
        "jaccard": 0.88
      }
    },
    {
      "type": "identifier_pattern",
      "description": "Target and candidate contain same identifier patterns (alphanumeric codes)",
      "score": 0.85,
      "details": {
        "shared_types": ["alphanumeric_code"]
      }
    },
    {
      "type": "structure_similarity",
      "description": "Single-level directory with flat file structure",
      "score": 0.90,
      "details": {
        "target_dirs": 0,
        "candidate_dirs": 1
      }
    }
  ],
  "alternatives": [
    {
      "path": "/Library/Audio/Vocaloid",
      "score": 0.42,
      "reason": "Similar extensions but fewer files and missing identifier patterns"
    },
    {
      "path": "/Library/Documents",
      "score": 0.18,
      "reason": "No extension overlap"
    }
  ],
  "uncertainty": [
    {
      "type": "insufficient_precedent",
      "description": "No existing category has this exact identifier pattern family"
    }
  ],
  "warnings": [],
  "model": "rule-based-v1.0",
  "created_at": 1700000000
}
```

### Key Fields

| Field | Description |
|-------|-------------|
| `target_path` | Absolute path of the target directory |
| `decision` | One of: `MOVE_EXISTING`, `CREATE_CATEGORY`, `LEAVE_UNCLASSIFIED`, `ASK_USER` |
| `selected_candidate` | Best candidate path (null for non-MOVE_EXISTING) |
| `confidence` | 0.0–1.0 score representing classifier confidence in the decision |
| `confidence_band` | "low", "medium", "high" — derived from confidence thresholds |
| `candidates_considered` | Number of candidates evaluated |
| `supporting_evidence` | Array of evidence comparisons supporting the top candidate |
| `alternatives` | Other candidates with scores and rejection reasons |
| `uncertainty` | List of uncertainty reasons reducing confidence |
| `warnings` | Non-fatal warnings (e.g., partial_scan on target/candidate) |
| `model` | Classifier identifier (e.g., "rule-based-v1.0" or "gemini-1.5-pro") |
| `created_at` | Unix timestamp |

### Decision Logic Clarification

**Score vs. Confidence are NOT the same:**

- `score` (in alternatives and supporting_evidence) = per-candidate similarity to target
- `confidence` = confidence in the overall decision
- `decision` is derived from score landscape, not a single threshold

**Decision rules:**

```text
best_score >= 0.8 AND best_score - second_score >= 0.1
    → MOVE_EXISTING (to best candidate)

best_score >= 0.4 AND best_score < 0.8
    → LEAVE_UNCLASSIFIED
    → alternatives lists all candidates above 0.3

best_score - second_score < 0.05  AND  best_score >= 0.4
    → ASK_USER (competing candidates are too close)

no candidates with score >= 0.3
    → if target has content → CREATE_CATEGORY
    → if target is empty → LEAVE_UNCLASSIFIED

target.partial_scan == true
    → always add warning to uncertainty
    → never MOVE_EXISTING (evidence incomplete)

any candidate.partial_scan == true
    → candidate gets warning in uncertainty
    → candidate score penalized by 0.2
```

This approach means:
- A single weak match → `LEAVE_UNCLASSIFIED`
- Two close matches → `ASK_USER`
- No match → `LEAVE_UNCLASSIFIED` or `CREATE_CATEGORY`

Confidence bands:
- "high": confidence >= 0.8
- "medium": 0.4 <= confidence < 0.8
- "low": confidence < 0.4

---

## 5. Separation of Concerns

### Current Layer (Phase 3) — Classification

- Consumes `DirectoryEvidence` (from Phase 2 scanner output)
- Produces `ClassificationResult`
- Does NOT modify filesystem
- Can be rule-based or AI-backed (both produce the same result schema)

### Phase 4 — Planning & Safe Actions (future, not in Phase 3)

```text
ClassificationResult
    →
Plan (proposed filesystem operations, with paths and validation checks)
    →
Validate (independently verify paths, conflicts, staleness)
    →
Preview (show what would happen)
    →
Apply (execute, only with explicit user consent)
    →
OperationLog (record before/after state)
    →
Undo (verify filesystem state before rollback)
```

The Plan/Validate/Preview/Apply/OperationLog/Undo pipeline is a separate concern from classification. It must validate AI-generated paths against the actual filesystem independently.

---

## 6. Architecture: Deterministic Core vs. AI/Provider Layer vs. MCP

### Deterministic Core (`src/classifier/`)

Contains:
- `ClassificationInput`, `CandidateEvidence`, `ClassificationResult` structs
- Supporting evidence structs: `EvidenceComparison`, `AlternativeCandidate`, `UncertaintyReason`, `Warning`
- `Decision` enum: `MOVE_EXISTING`, `CREATE_CATEGORY`, `LEAVE_UNCLASSIFIED`, `ASK_USER`
- Deterministic similarity computation functions (extension Jaccard, filename overlap, identifier overlap, structure comparison)
- `CandidateRanker` — pre-filters candidates by cheap heuristics
- `RuleBasedClassifier` — produces `ClassificationResult` from deterministic scores
- JSON Schema for `ClassificationResult`
- `ClassificationProcessor` trait — interface for pluggable classifiers

```rust
pub trait ClassificationProcessor {
    fn classify(&self, input: &ClassificationInput) -> Result<ClassificationResult>;
}
```

Does NOT depend on:
- Any AI provider API
- SQLite
- MCP
- Network
- Filesystem mutation

### AI/Provider Layer (Phase 4, not implemented)

Contains:
- Implementations of `ClassificationProcessor` for OpenAI, Anthropic, Gemini, Ollama
- Prompt template management
- Model response parsing into `ClassificationResult`
- Cost/latency tracking

The provider layer depends on the core, NOT vice versa. A provider can override:
- `confidence` value (its own calibrated score)
- `supporting_evidence` (AI-generated reasoning)
- `model` field

But must produce the same `ClassificationResult` JSON shape.

### MCP Adapter (Phase 6, future)

- Wraps the core classification as an MCP tool
- Does NOT become a core dependency
- Translates MCP requests into `ClassificationInput` and MCP responses from `ClassificationResult`

---

## 7. Minimal Phase 3 MVP

### What to implement

1. **Core types** (in `src/classifier/`):
   - `ClassificationInput`, `CandidateEvidence`, `ClassificationResult`
   - Supporting evidence structs: `EvidenceComparison`, `AlternativeCandidate`, `UncertaintyReason`, `Warning`
   - `Decision` enum: `MOVE_EXISTING`, `CREATE_CATEGORY`, `LEAVE_UNCLASSIFIED`, `ASK_USER`

2. **Deterministic comparison functions**:
   - `extension_similarity(target, candidate) -> f64`
   - `structure_similarity(target, candidate) -> f64`
   - `identifier_similarity(target, candidate) -> f64`
   - `filename_pattern_similarity(target, candidate) -> f64`
   - `candidate_ranking_score(target, candidate) -> f64`

3. **CandidateRanker** (deterministic pre-filter):
   - Takes target evidence + list of candidate evidence
   - Ranks by extension overlap + structure
   - Returns top `MAX_CANDIDATES` (default 20)

4. **RuleBasedClassifier** (implements `ClassificationProcessor`):
   - Uses deterministic similarity scores
   - Decision logic uses best_score, second_best_score, score_gap (NOT single threshold)
   - Handles partial_scan warnings
   - Always populates `supporting_evidence`, `alternatives`, `uncertainty`, `warnings`

5. **JSON Schema** for `ClassificationResult` at `schemas/classification-result.json`

6. **CLI command** `fi classify <target> --category-root <root>`:
   - Scans target dir
   - Scans category root children
   - Samples child evidence per candidate
   - Runs `RuleBasedClassifier`
   - Outputs `ClassificationResult` as JSON

7. **Tests** (in `tests/classifier_tests.rs`):
   - Test 1: target matching media fixture → `MOVE_EXISTING`
   - Test 2: target matching comics fixture → `MOVE_EXISTING`
   - Test 3: no candidates → `LEAVE_UNCLASSIFIED` or `CREATE_CATEGORY`
   - Test 4: competing candidates (similar scores) → `ASK_USER`
   - Test 5: empty target → `LEAVE_UNCLASSIFIED`
   - Test 6: partial_scan target → warning in `uncertainty`, never `MOVE_EXISTING`
   - Test 7: determinism (same input → same classification twice)
   - Test 8: no filesystem modification (scan before/after to verify)

### What NOT to implement in Phase 3 MVP

- Any AI provider integration (OpenAI, Anthropic, Gemini, etc.)
- MCP server/adapter
- SQLite or any database
- Filesystem mutation (move, copy, create, delete)
- GUI
- Embeddings
- Any provider-specific API calls
- Prompt engineering or model response parsing
- Operation log, planning, validation, preview, apply, undo
- Category-level aggregation (per-candidate evaluation only)
- Confidence calibration or statistical probability modeling

### What NOT to modify

- Phase 0/1/2 scanner code (no changes to `src/scanner.rs` behavior)
- Phase 2 evidence schema or types
- Phase 2 JSON Schema files
- Phase 2 tests
- Phase 0/1/2 fixtures

---

## 8. Definition of Done for Phase 3

```text
cargo test                              PASS
cargo clippy -- -D warnings            PASS
cargo build --release                  PASS

JSON Schema for ClassificationResult    PASS
Deterministic output                    PASS
No filesystem mutation                  PASS
No AI provider code                     PASS
No SQLite                               PASS
No MCP                                  PASS
No GUI                                  PASS

Files modified/created:
- src/classifier/mod.rs                 (new)
- src/classifier/types.rs               (new)
- src/classifier/comparison.rs          (new)
- src/classifier/ranker.rs              (new)
- src/classifier/rule_based.rs          (new)
- src/classifier/processor.rs           (new)
- schemas/classification-result.json    (new)
- tests/classifier_tests.rs             (new)
- docs/PHASE3.md                        (updated — this file)

Phase 4 (AI Interpretation Layer) may proceed after Phase 3 is validated.
```
