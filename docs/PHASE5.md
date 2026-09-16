# Phase 5 — Intent & Planning Layer (Frozen)

## Positioning in Architecture

Phases 0–4B form the **filesystem intelligence substrate**: scanning, evidence extraction, rule-based classification, and AI classification. Phase 4B's `ClassificationResult` is not an end product — it is a component within a larger reasoning pipeline.

Phase 5 adds the **intent-aware planning layer** on top:

```
                         User
                          │
                 Natural Language Request
                          │
                          ▼
                  ┌───────────────┐
                  │  Task Intent  │  ← 5A: parse & formalize
                  │ Understanding │
                  └───────┬───────┘
                          │
                          ▼
Filesystem ──→ Scanner ─→ Evidence
                           │
                           ▼
                  ┌───────────────┐
                  │ AI Reasoning  │  ← 5B: evidence analysis
                  │ & Analysis    │       (Phases 0–4B substrate)
                  └───────┬───────┘
                           │
            ┌──────────────┼──────────────┐
            │              │              │
            ▼              ▼              ▼
         Explain       Recommend      Ask User
            │              │              │
            └──────────────┼──────────────┘
                           ▼
                    Proposed Plan
                           │
                           ▼
                       Validate
                           │
                           ▼
                        Preview
                           │
                    User approval
                           │
                           ▼
                         Apply
                           │
                           ▼
                    Operation Log
                           │
                           ▼
                          Undo
```

## Core Abstraction

```rust
pub struct TaskIntent {
    pub goal: Goal,
    pub constraints: ConstraintSet,
    pub user_hints: HashMap<String, String>,
}

pub enum Goal {
    Organize { scope: PathBuf, purpose: String },
    Reorganize { scope: PathBuf, strategy: Option<String> },
    Clean { scope: PathBuf, rules: Vec<CleanRule> },
}

pub struct ConstraintSet {
    pub preserve_existing_folders: bool,
    pub merge_duplicates: bool,
    pub archive_old: Option<Duration>,
    pub auto_delete_temps: bool,
    pub max_interactive_questions: usize,
}

pub struct TaskAnalysis {
    pub intent: TaskIntent,
    pub evidence: Vec<FileSystemEvidence>,
    pub structure_analysis: StructureAnalysis,
    pub classification_results: Vec<ClassificationResult>,
    pub recommendations: Vec<Recommendation>,
    pub unknown_factors: Vec<UnknownFactor>,
    pub risk_assessment: RiskAssessment,
}

pub struct Recommendation {
    pub id: String,
    pub rationale: String,
    pub proposed_plan: OperationPlan,
    pub confidence: f32,
    pub warnings: Vec<String>,
}

pub struct OperationPlan {
    pub operations: Vec<FileSystemOperation>,
    pub dry_run: bool,
    pub estimated_impact: EstimatedImpact,
}

pub enum FileSystemOperation {
    Move { source: PathBuf, dest: PathBuf },
    CreateDir { path: PathBuf },
    Delete { path: PathBuf },
}

pub struct OperationLog {
    pub id: String,
    pub plan_id: String,
    pub entries: Vec<LogEntry>,
    pub started_at: u64,
    pub completed_at: Option<u64>,
    pub success_count: usize,
    pub failure_count: usize,
    pub skipped_count: usize,
    pub total_entries: usize,
}
```
## Sub-phase Details

### 5A — Intent Understanding (AI)

Parse natural language into `TaskIntent`. Identify known and unknown factors.

Example:
```
User: "Help me organize Downloads."
→ Goal::Organize { scope: "Downloads", purpose: "general organization" }
→ Unknown factors:
  - desired category structure
  - preserve existing folders?
  - handle duplicates?
  - delete temps?
```

### 5B — Evidence Analysis (Deterministic substrate, Phases 0–4B)

Run Scanner → Evidence on the scope. Then run multiple analyses:
- Classification (Phase 3/4B): classify target against existing categories
- Structure analysis: detect duplicates, redundant folders, inconsistent naming, mixed content
- Temporal analysis: age distribution of files
- Project structure detection: identify coherent project folders vs. loose files

### 5C — AI Reasoning & Recommendation (Deterministic + AI)

Synthesize TaskIntent + TaskAnalysis + ClassificationResult into a `Recommendation`.

**Input**: `TaskIntent` + `TaskAnalysis` (produced by 5B)

**Output**: `Recommendation` (NOT `OperationPlan` — that is 5E)

**Key boundary rules**:
- AI/recommender consumes evidence only — never modifies filesystem
- Constraints are **deterministically enforced** after recommendation generation:
  - `auto_delete_temps = false` → AI suggestions to delete temp files are blocked, not rejected
  - `preserve_existing_folders = true` → existing directory structure preserved
  - `archive_old = Some(duration)` → only archive if explicitly configured
- If no candidate categories exist, AI can propose new ones but cannot fabricate filesystem evidence
- Recommendation may produce `unresolved_questions` for 5D clarification
- If a constraint violation occurs, `constraint_violation` is set and the blocking operations are removed
- Provider failure or malformed AI response falls back to deterministic recommendation

```rust
pub struct Recommendation {
    pub id: String,
    pub strategy: RecommendationStrategy,
    pub rationale: String,                    // AI-generated explanation
    pub proposed_categories: Vec<ProposedCategory>,
    pub proposed_operations: Vec<ProposedOperation>,  // High-level, NOT executable paths
    pub unresolved_questions: Vec<ClarificationQuestion>,
    pub confidence: f64,                      // 0.0–1.0
    pub constraint_checks: Vec<ConstraintCheck>,     // Evidence constraints were verified
    pub constraint_violation: Option<ConstraintViolation>,  // Set if constraints were violated
    pub warnings: Vec<RecommendationWarning>,
    pub generated_at: u64,
}
```

Example:
```
I scanned Downloads. Found 5 major content types:
  - Documents (PDF, DOCX, XLSX): 37 files
  - Images (JPG, PNG): 82 files
  - Archives (ZIP, RAR): 23 files
  - Installers (EXE, MSI): 14 files
  - Development (PY, JS, etc.): 9 files

83 files are ambiguous (generic names, mixed extensions).

Recommended: create 5 category folders, move matched files,
leave ambiguous files in place.

Constraint violation: auto_delete_temps=false — temp file deletion blocked.
Unresolved: 2 questions for clarification.
Confidence: 0.73
```

### 5D — User Clarification (Deterministic + User)

Present unresolved questions + blocked operations to user, collect structured answers, update `TaskIntent` constraints, and re-run recommendation.

**Input**: `Recommendation` (from 5C) + `TaskAnalysis` (from 5B)

**Output**: Updated `Recommendation` with fewer unresolved questions (or user confirmation)

**Key boundary rules**:
- User answers are converted to structured `UserDecision`s, NOT free-form text fed back into prompts
- Each `UserDecision` maps to a `DecisionCategory` (PreserveExistingFolders, AutoDeleteTemps, etc.)
- Decisions are applied deterministically to update `TaskIntent` constraints
- Blocked operations (from constraint violations in 5C) are surfaced to the user — not silently stripped
- After applying decisions, recommendation is recomputed from the original `TaskAnalysis` (no re-scan)
- Clarification is bounded by `max_interactive_questions` from the intent constraints

```rust
pub struct UserDecision {
    pub category: DecisionCategory,
    pub answer: DecisionAnswer,        // Yes/No, Choice, Duration
    pub question_id: String,
    pub rationale: Option<String>,
}

pub enum DecisionCategory {
    PreserveExistingFolders,
    AutoDeleteTemps,
    MergeDuplicates,
    ArchiveOld,
    TaxonomyChoice,
    FileDisposition,
    DuplicateHandling,
    AmbiguousFileResolution,
    CleanupScopeDefinition,
}

pub struct ClarificationEngine;
impl ClarificationEngine {
    pub fn start(&self, rec: &Recommendation) -> Vec<ClarificationQuestion> { ... }
    pub fn blocked_operations(&self, rec: &Recommendation) -> Vec<String> { ... }
    pub fn resolve_question(&self, questions: &[ClarificationQuestion], id: &str, answer: DecisionAnswer) -> Result<UserDecision, ClarificationError> { ... }
    pub fn apply_decisions(&self, intent: &TaskIntent, decisions: &[UserDecision]) -> TaskIntent { ... }
    pub fn recompute_recommendation(&self, intent: &TaskIntent, analysis: &TaskAnalysis) -> Result<Recommendation, RecommendationError> { ... }
    pub fn is_complete(&self, rec: &Recommendation) -> bool { ... }
    pub fn summarize(&self, rec: &Recommendation) -> String { ... }
}
```

Decision loop:
```
Recommendation
      ↓
[ClarificationEngine::start]
      ↓
Unresolved questions + blocked operations
      ↓
User selects answers (structured)
      ↓
[ClarificationEngine::resolve_question]
      ↓
UserDecisions (structured, typed)
      ↓
[ClarificationEngine::apply_decisions]
      ↓
Updated TaskIntent (constraints updated)
      ↓
[ClarificationEngine::recompute_recommendation]
      ↓
New Recommendation with fewer questions
      ↓
Repeat until is_complete() = true, then proceed to 5E
```

### 5E — Operation Plan Generation (Deterministic resolver)

Translate approved `Recommendation` into concrete `OperationPlan` with `FileSystemOperation`s.

**Input**: `Recommendation` (approved) + `TaskAnalysis` (cached)

**Output**: `OperationPlan` (NOT applied — dry-run only)

**Key boundary rules**:
- **5E never executes filesystem operations** — only generates the plan
- Destination paths are generated **deterministically from category names**, not from AI
- AI provides `to_category` name (e.g., "document_storage") → 5E resolves to `scope.join("Documents")`
- Validation: source exists, destination within scope, no source=dest, no conflicting destinations
- All operations are dry-run by default
- Partial scan results generate validation warnings

```rust
pub enum FileSystemOperation {
    Move { source: PathBuf, dest: PathBuf },
    CreateDir { path: PathBuf },
    Delete { path: PathBuf, reason: String },
}

pub struct OperationPlan {
    pub id: String,
    pub recommendation_id: String,
    pub scope: PathBuf,
    pub operations: Vec<FileSystemOperation>,
    pub estimated_impact: EstimatedImpact,
    pub validation_warnings: Vec<ValidationWarning>,
    pub has_conflicts: bool,
    pub dry_run: bool,
    pub created_at: u64,
}
```

Example:
```
Operation Plan: plan-12345
Scope: /home/user/Downloads
Dry run: Yes

=== Create Directories ===
  + /home/user/Downloads/Documents
  + /home/user/Downloads/Images

=== File/Directory Moves ===
  /home/user/Downloads/doc1.pdf → /home/user/Downloads/Documents
  /home/user/Downloads/photo1.jpg → /home/user/Downloads/Images

=== Estimated Impact ===
  Files moved: 4
  Dirs created: 2
```

### 5F — Plan Validation & Preview (Deterministic)

Validate an `OperationPlan` against constraints and render a preview before execution.

**Input**: `OperationPlan` + `TaskIntent`

**Output**: `ValidationResult` + `PlanPreview` (textual preview)

**Key boundary rules**:
- Validates each operation: source exists, destination within scope, no source=dest collisions
- Checks constraint violations (temp file deletion, folder preservation)
- Detects duplicate sources, conflicting destinations, and circular dependencies
- Generates `ValidationStatus` per operation: `Valid`, `BlockedByConstraint`, `Conflict`, `Invalid`, `Warning`
- Produces `PlanPreview` — a human-readable summary with validation summary and per-operation status

```rust
pub enum ValidationStatus {
    Valid,
    BlockedByConstraint(String),
    Conflict(String),
    Invalid(String),
    Warning(String),
}

pub struct ValidationResult {
    pub plan_id: String,
    pub scope: PathBuf,
    pub validated_operations: Vec<ValidatedOperation>,
    pub summary: ValidationSummary,
    pub has_blocked: bool,
    pub has_conflicts: bool,
    pub has_invalid: bool,
    pub has_warnings: bool,
    pub executable_operations: usize,
}

pub struct PlanPreview;
impl PlanPreview {
    pub fn render(&self, plan: &OperationPlan, validation: &ValidationResult) -> String;
}
```

### 5G — Apply, OperationLog & Undo (Deterministic executor)

Execute validated `OperationPlan` on the filesystem, log each operation's outcome, and provide deterministic undo capability.

**Input**: `OperationPlan` (validated) + `ValidationResult` (validated)

**Output**: `ApplyResult` containing an `OperationLog` with per-operation entries; `UndoResult` for undo operations

**Key boundary rules**:
- **Only validated plans are accepted** — `dry_run` plans are rejected
- Each operation transitions to a terminal `ExecutionStatus`: `Success`, `Failed`, `Skipped`, or `UndoNotSupported`
- `OperationLog` records per-operation metadata: source, target, status, timestamps, error, undo capability
- JSON/in-memory logging (no SQLite) — `OperationLog` is serializable
- Move and CreateDir operations are undoable; Delete operations are not (irreversible)
- Undo reverses only `Success` operations; preconditions are checked (source must exist, target must not exist)
- Undo conflicts are reported via `UndoConflict` enum: `SourceMissing`, `TargetExists`, `NotUndoable`
- Dry-run flag on `Apply` produces no filesystem changes
- `apply` CLI loads a plan JSON file, validates it, and executes
- `undo` CLI loads a log JSON file and reverses executed operations

```rust
pub enum ExecutionStatus {
    Pending,
    Success,
    Failed(String),
    Skipped(String),
    UndoNotSupported,
}

pub struct LogEntry {
    pub id: String,
    pub plan_id: String,
    pub operation_type: String,
    pub original_source: PathBuf,
    pub applied_target: PathBuf,
    pub status: ExecutionStatus,
    pub started_at: u64,
    pub completed_at: Option<u64>,
    pub error: Option<String>,
    pub undo_supported: bool,
}

pub struct OperationLog {
    pub id: String,
    pub plan_id: String,
    pub entries: Vec<LogEntry>,
    pub started_at: u64,
    pub completed_at: Option<u64>,
    pub success_count: usize,
    pub failure_count: usize,
    pub skipped_count: usize,
    pub total_entries: usize,
}

pub struct UndoResult {
    pub log_id: String,
    pub applied_undoes: Vec<UndoLogEntry>,
    pub conflicts: Vec<UndoConflict>,
    pub total_undo_operations: usize,
    pub completed_at: u64,
}

pub struct Executor;
impl Executor {
    pub fn execute(&self, plan: &OperationPlan, validation: &ValidationResult, dry_run: bool) -> Result<ApplyResult, ApplyError>;
    pub fn undo(&self, log: &OperationLog) -> Result<UndoResult, ApplyError>;
}
```

CLI commands:
```bash
# Validate and preview a plan (5F)
fi validate "Organize Downloads by category" --scope ./Downloads

# Apply a validated plan (5G)
fi apply --plan plan.json --dry-run    # Dry run: no filesystem changes
fi apply --plan plan.json              # Execute: applies validated operations
fi apply --plan plan.json --force      # Force apply: bypasses BLOCKED ops (constraint violations) only; INVALID ops (missing source, path outside scope) are always rejected

# Undo executed operations (5G)
fi undo --log log.json                 # Reverse all undoable operations
fi undo --log log.json --dry-run       # Preview undo actions
```

| Component | Owner | Reason |
|-----------|-------|--------|
| Intent parsing | AI | Natural language understanding |
| Evidence extraction | Deterministic | Scanner is deterministic |
| Classification | AI + Deterministic baseline | Rule-based baseline + AI refinement |
| Structure analysis | Deterministic | Pattern matching on evidence |
| Recommendation synthesis | Deterministic + AI | Interpret evidence, explain rationale |
| Constraint enforcement | Deterministic | Verify proposed operations against intent constraints |
| Clarification questions | Deterministic | Generated from evidence gaps |
| User decision mapping | Deterministic | Structured answer → constraint mapping |
| Recomputation | Deterministic + AI | Re-run recommendation with updated intent |
| User clarification | User | Resolve unknowns |
| Plan generation | Deterministic resolver (5E) | Path resolution from category names, validation, dry-run — NO filesystem execution |
| Plan validation | Deterministic | Check constraints, no data loss |
| Plan preview | Deterministic | Render before/after tree |
| Apply | User approval + Deterministic executor | Safety: requires explicit approval |
| Undo | Deterministic | Replay inverse operations from log |

## Relationship to Phases 0–4B

- **Phase 0–2 (Scanner/Evidence)**: Substrate — unchanged
- **Phase 3 (Classification)**: Substrate — `ClassificationResult` used as one input to AI reasoning
- **Phase 4A (AI interface)**: Substrate — `AiClassifier` trait used by recommendation engine
- **Phase 4B (OpenAI provider)**: Substrate — concrete provider for AI reasoning
- **Phase 5**: Orchestration layer — ties substrate into user-facing agent

## CLI Interface (Phase 5)

```bash
# Parse intent only
fi intent "Help me organize Downloads"

# Analyze filesystem + evidence
fi analyze "Organize Downloads" --scope ./Downloads

# Generate recommendation (5C)
fi recommend "Organize Downloads by category" --scope ./Downloads

# Show summary with questions + blocked operations (5D)
fi clarify "Organize Downloads" --scope ./Downloads

# Generate operation plan from recommendation (5E)
fi plan "Organize Downloads by category" --scope ./Downloads

# Interactive mode (planned for 5D+5E)
fi plan "Organize by Work / Personal / Entertainment" --interactive

# Validate and preview a plan (5F)
fi validate "Organize Downloads by category" --scope ./Downloads
fi validate "Organize Downloads by category" --scope ./Downloads --dry-run

# Apply a validated plan (5G)
fi apply --plan plan.json --dry-run    # Dry run: no filesystem changes
fi apply --plan plan.json              # Execute: applies validated operations
fi apply --plan plan.json --force      # Force apply: bypasses BLOCKED ops (constraint violations) only; INVALID ops (missing source, path outside scope) are always rejected

# Undo executed operations (5G)
fi undo --log log.json                 # Reverse all undoable operations
fi undo --log log.json --dry-run       # Preview undo actions
```

## What NOT to implement in Phase 5

- New evidence scanning (reuse Phase 0–2)
- New classification logic (reuse Phase 3/4B)
- New AI provider integration (reuse Phase 4B)
- Multiple simultaneous task intents (single focus per invocation)
- Long-term conversation state / memory

## Phase Status

**Phase 5 Frozen** — All sub-phases complete. No further features to be added within Phase 5.

| Sub-phase | Status | Implementation |
|-----------|--------|----------------|
| 5A — Intent Understanding | Complete | `src/agent/intent.rs` |
| 5B — Evidence Analysis | Complete | `src/agent/analysis.rs` |
| 5C — AI Reasoning & Recommendation | Complete | `src/agent/recommendation.rs` |
| 5D — User Clarification | Complete | `src/agent/clarification.rs` |
| 5E — Operation Plan Generation | Complete | `src/agent/plan.rs` |
| 5F — Plan Validation & Preview | Complete | `src/agent/validate.rs` |
| 5G — Apply, OperationLog & Undo | Complete | `src/agent/executor.rs` |

## Authority Separation Boundary

```
AI
 │
 │ proposes
 ▼
Recommendation
 │
 ▼
OperationPlan
 │
 ▼
5F Validation
 │
 ├── INVALID ──────→ always rejected (never bypassable)
 ├── CONFLICT ─────→ always rejected (never bypassable)
 ├── BLOCKED ──────→ --force bypasses only as SKIPPED
 └── VALID
       │
       ▼
5G live precondition check
       │
       ▼
Filesystem mutation
       │
       ▼
OperationLog (serializable)
       │
       ▼
Undo successful/reversible operations only
```

> **AI has reasoning authority, but no filesystem mutation authority.**
> Deterministic validation/execution layer is the sole authority for filesystem mutations.
