# Phase 3 — Evidence-Driven Classification Design

## Status: DESIGN ONLY — No Implementation Yet

---

## Core Principle

> **Filesystem is the source of truth. Evidence is observation. Interpretation belongs downstream.**

The classifier must **not** modify the filesystem. It produces a **Classification Result** that feeds into a separate **Plan** layer.

---

## 1. Classification Input Model

### 1.1 Target Directory Evidence

```rust
/// The unclassified directory to be classified.
/// Reuses existing DirectoryEvidence from Phase 2.
pub struct TargetEvidence {
    pub evidence: DirectoryEvidence,
    /// Optional: user-provided hints (e.g., "this is audio")
    pub user_hints: Option<UserHints>,
}
```

### 1.2 Candidate Directory Evidence

```rust
/// An existing directory that serves as a classification precedent.
/// Contains its own evidence plus aggregated child evidence.
pub struct CandidateEvidence {
    /// The candidate directory itself (e.g., "音聲/")
    pub directory: DirectoryEvidence,
    /// Aggregated evidence from its immediate children (e.g., RJ01547001/, RJ01547002/, ...)
    /// Bounded by max_candidate_children limit.
    pub children: Vec<DirectoryEvidence>,
    /// Summary statistics for quick filtering
    pub summary: CandidateSummary,
}
```

### 1.3 Candidate Summary

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateSummary {
    /// Total number of child directories
    pub child_count: usize,
    /// Aggregated extension histogram across all children
    pub aggregate_extension_histogram: HashMap<String, u64>,
    /// Most common extension across children
    pub dominant_extension: Option<String>,
    /// Average file count per child
    pub avg_file_count: f64,
    /// Average directory count per child
    pub avg_directory_count: f64,
    /// Whether any children are empty
    pub has_empty_children: bool,
    /// Common identifier types found in children
    pub common_identifier_types: Vec<IdentifierType>,
    /// Depth of this candidate from scan root
    pub depth: usize,
}
```

### 1.4 Filesystem Precedent Context

```rust
/// The complete classification context provided to the classifier.
pub struct ClassificationInput {
    /// The target directory to classify
    pub target: TargetEvidence,
    /// All candidate directories with their evidence
    pub candidates: Vec<CandidateEvidence>,
    /// Scan metadata for provenance
    pub scan_metadata: ScanMetadata,
    /// Configuration for this classification run
    pub config: ClassificationConfig,
}
```

### 1.5 Classification Configuration

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationConfig {
    /// Maximum number of candidate children to include per candidate
    pub max_candidate_children: usize,
    /// Minimum confidence threshold for MOVE_EXISTING
    pub move_existing_threshold: f64,
    /// Minimum confidence threshold for CREATE_CATEGORY
    pub create_category_threshold: f64,
    /// Whether to include empty directories as candidates
    pub include_empty_candidates: bool,
    /// Maximum total candidates to consider
    pub max_candidates: usize,
}

impl Default for ClassificationConfig {
    fn default() -> Self {
        Self {
            max_candidate_children: 50,
            move_existing_threshold: 0.85,
            create_category_threshold: 0.70,
            include_empty_candidates: true,
            max_candidates: 20,
        }
    }
}
```

---

## 2. Candidate Representation

### 2.1 How Existing Directories Become Precedents

1. **Scan the entire filesystem** (or relevant subtree) using the Phase 2 scanner
2. **Identify candidate directories**: Directories at a specific depth that contain subdirectories (the "category" level)
   - Example: `音聲/`, `漫畫/`, `空資料夾/` are candidates
   - Their children (`RJ01547001/`, `volume01/`) are the precedents
3. **For each candidate**, collect:
   - The candidate's own `DirectoryEvidence`
   - Evidence from up to `max_candidate_children` of its immediate subdirectories
   - Compute `CandidateSummary` for quick filtering

### 2.2 Empty Directory Handling

- Empty directories (`is_empty == true`) are valid candidates
- They represent the "no precedent" case
- Can be matched when target is also empty
- Configurable via `include_empty_candidates`

### 2.3 Candidate Evidence Bounding

- Limit children per candidate: `max_candidate_children` (default 50)
- Limit total candidates: `max_candidates` (default 20)
- Selection strategy: deterministic (first N by name, or most representative)

---

## 3. Comparison / Evidence Model

### 3.1 Evidence Comparison Types

The classifier evaluates similarity across multiple dimensions:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EvidenceComparison {
    /// Extension distribution similarity (Jensen-Shannon divergence or cosine)
    ExtensionSimilarity {
        observed: String,
        score: f64,
        target_histogram: HashMap<String, u64>,
        candidate_histogram: HashMap<String, u64>,
    },
    /// File count similarity (ratio of file counts)
    FileCountSimilarity {
        observed: String,
        score: f64,
        target_count: u64,
        candidate_avg_count: f64,
    },
    /// Directory structure similarity
    StructureSimilarity {
        observed: String,
        score: f64,
        target_dir_count: u64,
        candidate_avg_dir_count: f64,
    },
    /// Filename pattern similarity (from filename_sample)
    FilenamePatternSimilarity {
        observed: String,
        score: f64,
        target_samples: Vec<String>,
        candidate_samples: Vec<String>,
    },
    /// Syntactic identifier type overlap
    IdentifierSimilarity {
        observed: String,
        score: f64,
        target_types: Vec<IdentifierType>,
        candidate_types: Vec<IdentifierType>,
    },
    /// Identifier value pattern similarity (e.g., same RJxxxx pattern)
    IdentifierPatternSimilarity {
        observed: String,
        score: f64,
        target_patterns: Vec<String>,
        candidate_patterns: Vec<String>,
    },
    /// Text file presence similarity
    TextFileSimilarity {
        observed: String,
        score: f64,
        target_presence: TextFilePresence,
        candidate_presence: TextFilePresence,
    },
    /// Depth/context similarity
    DepthSimilarity {
        observed: String,
        score: f64,
        target_depth: usize,
        candidate_depth: usize,
    },
    /// Empty directory match
    EmptyMatch {
        observed: String,
        score: f64,
    },
}
```

### 3.2 Deterministic Core Comparisons

These can be computed **without AI** and provide baseline scores:

| Comparison | Method | Output |
|------------|--------|--------|
| Extension Similarity | Jensen-Shannon divergence on normalized histograms | 0.0–1.0 |
| File Count Similarity | `1 - |log(target) - log(candidate_avg)| / max(log)` | 0.0–1.0 |
| Structure Similarity | Same as file count but for directories | 0.0–1.0 |
| Identifier Type Overlap | Jaccard index on identifier type sets | 0.0–1.0 |
| Text File Presence | Jaccard on boolean flags | 0.0–1.0 |
| Depth Similarity | `1 - |target - candidate| / max_depth` | 0.0–1.0 |
| Empty Match | 1.0 if both empty, 0.0 otherwise | 0.0 or 1.0 |

### 3.3 AI/Provider Layer Comparisons

These require semantic understanding:

| Comparison | Input | Provider Role |
|------------|-------|---------------|
| Filename Pattern Similarity | `filename_sample` from target + candidates | Recognize naming conventions (sequential, hashed, dated, etc.) |
| Identifier Pattern Similarity | `syntactic_identifiers` values | Recognize same identifier scheme (RJxxxxx, ISBN, UUID, etc.) |
| Semantic Category Inference | All evidence combined | Infer "this is audio", "this is comics", etc. |

---

## 4. Classification Result Schema

### 4.1 Decision Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationDecision {
    /// Move target into an existing candidate directory
    MoveExisting,
    /// Create a new category directory (sibling to candidates)
    CreateCategory,
    /// Leave unclassified — insufficient evidence
    LeaveUnclassified,
    /// Ask user for guidance
    AskUser,
}
```

### 4.2 Supporting Evidence

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportingEvidence {
    /// The comparison that supports this decision
    pub comparison: EvidenceComparison,
    /// Weight of this evidence in the final decision (0.0–1.0)
    pub weight: f64,
    /// Human-readable explanation
    pub explanation: String,
}
```

### 4.3 Alternative Candidate

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeCandidate {
    /// The candidate that was considered but not selected
    pub candidate_path: PathBuf,
    /// Candidate's summary
    pub summary: CandidateSummary,
    /// Why it was rejected
    pub rejection_reason: String,
    /// Confidence it would have been correct (for analysis)
    pub hypothetical_confidence: f64,
    /// Evidence comparisons for this candidate
    pub evidence: Vec<EvidenceComparison>,
}
```

### 4.4 Uncertainty / Abstention Reasons

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UncertaintyReason {
    /// No candidates available
    NoCandidates,
    /// All candidates below threshold
    AllBelowThreshold,
    /// Multiple candidates with similar scores
    Ambiguous { top_candidates: Vec<PathBuf>, score_spread: f64 },
    /// Target is empty but no empty candidates
    EmptyTargetNoEmptyCandidate,
    /// Target has unique identifier pattern not seen in candidates
    NovelIdentifierPattern,
    /// Target extension distribution doesn't match any candidate
    NovelExtensionDistribution,
    /// Insufficient scan data (partial scan)
    PartialScan,
    /// Confidence below ASK_USER threshold
    LowConfidence,
}
```

### 4.5 Complete Classification Result

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResult {
    /// The decision
    pub decision: ClassificationDecision,
    /// Selected candidate path (for MOVE_EXISTING)
    pub selected_candidate: Option<PathBuf>,
    /// Proposed new category name (for CREATE_CATEGORY)
    pub proposed_category_name: Option<String>,
    /// Overall confidence (0.0–1.0)
    pub confidence: f64,
    /// Supporting evidence for the decision
    pub supporting_evidence: Vec<SupportingEvidence>,
    /// Alternative candidates considered
    pub alternatives: Vec<AlternativeCandidate>,
    /// Uncertainty reasons (if not confident)
    pub uncertainty: Vec<UncertaintyReason>,
    /// Classification timestamp
    pub classified_at: u64,
    /// Schema version
    pub schema_version: String,
    /// Provider used (if AI-assisted)
    pub provider: Option<String>,
    /// Provider model (if AI-assisted)
    pub model: Option<String>,
}
```

---

## 5. Separation of Concerns

### 5.1 Layer Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        USER INTERFACE                           │
│  (CLI, GUI, MCP, API)                                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                        PLAN LAYER                               │
│  - Takes ClassificationResult                                   │
│  - Generates execution plan (move, create, skip)                │
│  - Validates plan (no conflicts, permissions, space)            │
│  - Produces preview (dry-run)                                   │
│  - Executes plan (with transaction log)                         │
│  - Supports undo via operation log                              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                   CLASSIFICATION LAYER                          │
│  - Takes ClassificationInput                                    │
│  - Produces ClassificationResult                                │
│  - NO filesystem mutation                                       │
│  - Deterministic core + optional AI provider                    │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      EVIDENCE LAYER (Phase 2)                   │
│  - Scanner produces DirectoryEvidence                           │
│  - ScanMetadata for provenance                                  │
│  - JSONL output                                                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      FILESYSTEM                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 5.2 Plan Layer (Future)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub operations: Vec<PlanOperation>,
    pub validation: PlanValidation,
    pub preview: PlanPreview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlanOperation {
    Move { from: PathBuf, to: PathBuf },
    CreateDirectory { path: PathBuf },
    Skip { path: PathBuf, reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanValidation {
    pub valid: bool,
    pub conflicts: Vec<Conflict>,
    pub warnings: Vec<String>,
    pub required_permissions: Vec<PathBuf>,
    pub estimated_space: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanPreview {
    pub operations_summary: String,
    pub affected_paths: Vec<PathBuf>,
    pub dry_run_output: String,
}
```

### 5.3 Operation Log & Undo (Future)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationLog {
    pub operations: Vec<LoggedOperation>,
    pub started_at: u64,
    pub completed_at: Option<u64>,
    pub status: OperationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggedOperation {
    pub operation: PlanOperation,
    pub executed_at: u64,
    pub success: bool,
    pub error: Option<String>,
    /// For undo: reverse operation
    pub reverse: Option<PlanOperation>,
}
```

---

## 6. Deterministic Core vs AI/Provider Layer vs MCP Adapter

### 6.1 Deterministic Core (Rust, No External Dependencies)

| Component | Responsibility |
|-----------|----------------|
| `EvidenceComparator` | Compute all deterministic similarity scores |
| `CandidateSelector` | Filter and rank candidates by deterministic scores |
| `ThresholdEvaluator` | Apply confidence thresholds |
| `DecisionEngine` | Combine scores → preliminary decision |
| `UncertaintyAnalyzer` | Identify why confidence is low |

**Output**: `ClassificationResult` with `provider: None`, based purely on evidence math.

### 6.2 AI/Provider Layer (Pluggable, Optional)

```rust
pub trait ClassificationProvider: Send + Sync {
    /// Enhance deterministic result with semantic understanding
    fn enhance(
        &self,
        input: &ClassificationInput,
        deterministic_result: &ClassificationResult,
    ) -> Result<ClassificationResult>;

    /// Provider identifier
    fn name(&self) -> &str;

    /// Model identifier
    fn model(&self) -> &str;
}
```

**Providers**: Gemini, Claude, GPT, Ollama, local rule-based, etc.

**Input to provider**: Full `ClassificationInput` + deterministic `ClassificationResult`

**Output from provider**: Enhanced `ClassificationResult` with:
- Adjusted confidence
- Additional `SupportingEvidence` with semantic explanations
- Refined decision (e.g., deterministic says `ASK_USER`, provider says `MOVE_EXISTING` with high confidence)
- `provider` and `model` fields populated

### 6.3 MCP Adapter (Future)

```rust
/// MCP tool definition for classification
pub struct ClassifyTool {
    pub name: "classify_directory",
    pub description: "Classify a target directory against existing candidates",
    pub input_schema: ClassificationInputSchema,  // JSON Schema
    pub output_schema: ClassificationResultSchema, // JSON Schema
}
```

- MCP adapter wraps the Classification Layer
- Exposes `classify_directory` tool
- Handles serialization/deserialization
- No business logic in adapter

---

## 7. Minimal Phase 3 MVP

### 7.1 What IS in MVP

1. **`ClassificationInput` construction**
   - Scan target directory → `TargetEvidence`
   - Scan candidate directories → `Vec<CandidateEvidence>`
   - Build `ClassificationInput`

2. **Deterministic `EvidenceComparator`**
   - All 7 deterministic comparisons from §3.2
   - Unit tested with fixtures

3. **`CandidateSelector`**
   - Filter by depth proximity
   - Rank by aggregate deterministic score
   - Apply `max_candidates` limit

4. **`ThresholdEvaluator` + `DecisionEngine`**
   - Apply thresholds from `ClassificationConfig`
   - Produce preliminary `ClassificationDecision`

5. **`UncertaintyAnalyzer`**
   - Generate `UncertaintyReason` enum values

6. **`ClassificationResult` serialization**
   - JSON output (single object, not JSONL)
   - Schema validation

7. **CLI command: `fi classify`**
   - `--target <path>`
   - `--candidates <path>` (directory containing category dirs)
   - `--config <path>` (optional TOML)
   - `--provider <name>` (optional, for future)
   - Outputs `ClassificationResult` as JSON

8. **Integration tests with fixtures**
   - Test each decision type
   - Test uncertainty reasons
   - Test deterministic reproducibility

### 7.2 What is NOT in MVP

| Feature | Deferred To |
|---------|-------------|
| AI/Provider integration | Phase 3.1 |
| Plan layer (move, create, preview, apply) | Phase 4 |
| Operation log & undo | Phase 4 |
| MCP adapter | Phase 5 |
| GUI | Never (separate project) |
| SQLite persistence | Phase 4+ |
| Batch classification (multiple targets) | Phase 3.1 |
| Learning from user feedback | Phase 4+ |
| Cross-scan precedent reuse | Phase 4+ |
| Custom identifier types | Phase 3.1 |

---

## 8. JSON Schemas

### 8.1 Classification Input Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://folder-intelligence.org/schemas/classification-input.json",
  "title": "Classification Input",
  "type": "object",
  "required": ["target", "candidates", "scan_metadata", "config"],
  "properties": {
    "target": { "$ref": "directory-evidence.json" },
    "candidates": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["directory", "children", "summary"],
        "properties": {
          "directory": { "$ref": "directory-evidence.json" },
          "children": { "type": "array", "items": { "$ref": "directory-evidence.json" } },
          "summary": { "$ref": "#/$defs/candidate_summary" }
        }
      }
    },
    "scan_metadata": { "$ref": "scan-metadata.json" },
    "config": { "$ref": "#/$defs/classification_config" }
  },
  "$defs": {
    "candidate_summary": { ... },
    "classification_config": { ... }
  }
}
```

### 8.2 Classification Result Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://folder-intelligence.org/schemas/classification-result.json",
  "title": "Classification Result",
  "type": "object",
  "required": ["decision", "confidence", "supporting_evidence", "alternatives", "uncertainty", "classified_at", "schema_version"],
  "properties": {
    "decision": { "enum": ["move_existing", "create_category", "leave_unclassified", "ask_user"] },
    "selected_candidate": { "type": ["string", "null"] },
    "proposed_category_name": { "type": ["string", "null"] },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "supporting_evidence": { "type": "array", "items": { "$ref": "#/$defs/supporting_evidence" } },
    "alternatives": { "type": "array", "items": { "$ref": "#/$defs/alternative_candidate" } },
    "uncertainty": { "type": "array", "items": { "$ref": "#/$defs/uncertainty_reason" } },
    "classified_at": { "type": "integer", "minimum": 0 },
    "schema_version": { "const": "3.0.0" },
    "provider": { "type": ["string", "null"] },
    "model": { "type": ["string", "null"] }
  },
  "$defs": {
    "supporting_evidence": { ... },
    "alternative_candidate": { ... },
    "uncertainty_reason": { ... },
    "evidence_comparison": { ... }
  }
}
```

---

## 9. Module Structure Proposal

```
src/
├── classification/
│   ├── mod.rs              # Public API
│   ├── input.rs            # ClassificationInput, CandidateEvidence, Config
│   ├── comparator.rs       # EvidenceComparator (deterministic)
│   ├── selector.rs         # CandidateSelector
│   ├── decision.rs         # DecisionEngine, ThresholdEvaluator
│   ├── uncertainty.rs      # UncertaintyAnalyzer
│   ├── result.rs           # ClassificationResult, SupportingEvidence, etc.
│   ├── provider.rs         # ClassificationProvider trait
│   └── cli.rs              # CLI command integration
├── evidence.rs             # (existing)
├── scanner.rs              # (existing)
├── cli.rs                  # (existing, add classify subcommand)
└── lib.rs                  # (re-export classification)
```

---

## 10. Test Strategy

### 10.1 Unit Tests (Deterministic Core)

| Test | Fixture | Expected |
|------|---------|----------|
| Extension similarity | media vs comics | Low score (~0.1) |
| Extension similarity | media vs media | High score (~0.95) |
| File count similarity | 50 files vs avg 48 | High score |
| Identifier overlap | ISBN + UUID vs ISBN only | Medium score |
| Empty match | empty target + empty candidate | 1.0 |
| Depth similarity | depth 2 vs depth 2 | 1.0 |

### 10.2 Integration Tests (Full Classification)

| Scenario | Target | Candidates | Expected Decision |
|----------|--------|------------|-------------------|
| Clear audio match | 48 mp3, 1 jpg, 2 txt | 音聲/ (many mp3 folders) | MOVE_EXISTING, high confidence |
| Clear comics match | 5 cbz, 1 xml | 漫畫/ (many cbz folders) | MOVE_EXISTING, high confidence |
| Empty target | empty | 空資料夾/ (empty) | MOVE_EXISTING |
| Novel type | 10 pdf, 5 epub | 音聲/, 漫畫/ | CREATE_CATEGORY or ASK_USER |
| Ambiguous | 10 mp3, 10 pdf | 音聲/, 文檔/ | ASK_USER (ambiguous) |
| No candidates | any | (none) | LEAVE_UNCLASSIFIED |

### 10.3 Determinism Tests

- Same input → identical `ClassificationResult` (provider = None)
- Repeated runs produce byte-identical JSON output

---

## 11. Configuration File (TOML)

```toml
# classification.toml
[classification]
max_candidate_children = 50
max_candidates = 20
move_existing_threshold = 0.85
create_category_threshold = 0.70
include_empty_candidates = true

[comparison_weights]
extension_similarity = 0.35
file_count_similarity = 0.15
structure_similarity = 0.10
filename_pattern_similarity = 0.15
identifier_similarity = 0.15
text_file_similarity = 0.05
depth_similarity = 0.05
```

---

## 12. Acceptance Criteria for Phase 3 MVP

- [ ] `cargo test` — all unit + integration tests pass
- [ ] `cargo clippy -- -D warnings` — clean
- [ ] `cargo build --release` — succeeds
- [ ] `fi classify --target fixtures/media --candidates fixtures` → valid JSON output
- [ ] ClassificationResult validates against JSON schema
- [ ] Deterministic: 10 repeated runs produce identical output
- [ ] No filesystem mutation occurs
- [ ] No AI/provider code in deterministic path
- [ ] Phase 0/1/2 behavior unchanged (scanner tests still pass)

---

## 13. Future Extensions (Post-MVP)

### 13.1 Phase 3.1 — Provider Integration
- Implement `ClassificationProvider` for Gemini/Claude/Ollama
- Prompt engineering for evidence-to-decision
- Provider result merging with deterministic result

### 13.2 Phase 4 — Plan & Execute
- Plan layer with validation, preview, apply
- Operation log with undo
- Transactional moves (atomic where possible)

### 13.3 Phase 5 — MCP & Ecosystem
- MCP server exposing `classify_directory`, `plan_moves`, `execute_plan`
- Language bindings (Python, Node.js, Go)
- VS Code extension

---

## Appendix: Example Classification Result

```json
{
  "decision": "move_existing",
  "selected_candidate": "/home/user/音聲",
  "proposed_category_name": null,
  "confidence": 0.94,
  "supporting_evidence": [
    {
      "comparison": {
        "type": "extension_similarity",
        "observed": "Target: 94% mp3 (48/51). Candidate children: avg 91% mp3.",
        "score": 0.97,
        "target_histogram": { "mp3": 48, "jpg": 1, "txt": 2 },
        "candidate_histogram": { "mp3": 455, "jpg": 12, "txt": 33 }
      },
      "weight": 0.35,
      "explanation": "Extension distribution nearly identical to audio category"
    },
    {
      "comparison": {
        "type": "identifier_pattern_similarity",
        "observed": "Target has RJ01547914 pattern. Candidate children all have RJxxxxxxxx pattern.",
        "score": 0.99,
        "target_patterns": ["RJ01547914"],
        "candidate_patterns": ["RJ01547001", "RJ01547002", "RJ01547123"]
      },
      "weight": 0.15,
      "explanation": "Same identifier scheme (RJ code) used in candidate category"
    },
    {
      "comparison": {
        "type": "file_count_similarity",
        "observed": "Target: 51 files. Candidate children avg: 49 files.",
        "score": 0.96,
        "target_count": 51,
        "candidate_avg_count": 49.2
      },
      "weight": 0.15,
      "explanation": "File count matches typical audio release folder size"
    }
  ],
  "alternatives": [
    {
      "candidate_path": "/home/user/漫畫",
      "summary": { "child_count": 34, "dominant_extension": "cbz", ... },
      "rejection_reason": "Extension distribution mismatch (target: mp3, candidate: cbz)",
      "hypothetical_confidence": 0.02,
      "evidence": [...]
    }
  ],
  "uncertainty": [],
  "classified_at": 1700000000,
  "schema_version": "3.0.0",
  "provider": null,
  "model": null
}
```

---

*End of Phase 3 Design Document*