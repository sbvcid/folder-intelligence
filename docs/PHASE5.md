# Phase 5 — Intent & Planning Layer

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
    pub risks: Vec<Risk>,
    pub alternatives: Vec<AlternativePlan>,
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
    Copy { source: PathBuf, dest: PathBuf },
    Rename { from: PathBuf, to: PathBuf },
}

pub struct OperationLog {
    pub plan_id: Uuid,
    pub operations: Vec<LoggedOperation>,
    pub applied_at: DateTime<Utc>,
    pub reversible: bool,
    pub undo_script: UndoScript,
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

### 5E — Operation Plan Generation (AI)

Generate `OperationPlan` with concrete filesystem operations. Dry-run by default.

### 5F — Validation & Preview (Deterministic)

Validate the plan: no conflicting operations, paths exist, no data loss.
Preview the plan: show before/after tree, impacted file counts.

### 5G — Apply / Undo (User approval + Deterministic)

Only with user approval. Execute operations, record `OperationLog`, generate undo script.

## Layering: What's Deterministic vs AI vs User

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
| Plan generation | AI (5E) | Translate approved recommendations to operations |
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
# Natural language request
fi plan "Help me organize Downloads"

# Interactive mode
fi plan "Organize by Work / Personal / Entertainment" --interactive

# Show analysis only
fi plan "Organize Downloads" --analyze-only

# Apply with approval
fi plan "Organize Downloads" --apply
```

## What NOT to implement in Phase 5

- New evidence scanning (reuse Phase 0–2)
- New classification logic (reuse Phase 3/4B)
- New AI provider integration (reuse Phase 4B)
- Multiple simultaneous task intents (single focus per invocation)
- Long-term conversation state / memory
