# Phase 0–5 Architecture Audit Report

## 1. Architecture Findings

### 1.1 Overall Structure

The project is a single Rust crate (`folder-intelligence`) containing both a library (`src/lib.rs`) and a binary (`src/main.rs` → `fi`). The crate uses `pico-args` for CLI parsing (no `clap` or `structopt`). Architecture layers:

```
src/
├── main.rs           # Binary entry point (13 lines, delegates to cli.run())
├── lib.rs            # Library facade — flat re-export of all public types
├── cli.rs            # CLI orchestrator (509 lines, 13 commands)
├── evidence.rs       # Phase 0–2: Core data model (DirectoryEvidence, ScanLimits, etc.)
├── scanner.rs        # Phase 0–2: Iterative directory scanner (1494 lines, 15% tests)
├── classification/   # Phase 3/4B: File classification
│   ├── input.rs      # ClassificationInput, ClassificationConfig
│   ├── comparator.rs # EvidenceComparator — 7 scoring dimensions
│   ├── selector.rs   # CandidateSelector — weighted scoring
│   ├── decision.rs   # DecisionEngine — threshold-based decisions
│   ├── processor.rs  # RuleBasedProcessor — trait wrapper
│   ├── ai_provider.rs# AiClassifier trait, RealAiClassifier enum
│   ├── mock_ai.rs    # MockAiClassifier — deterministic AI simulation
│   └── openai_provider.rs (#[cfg(feature="network")]) # OpenAI HTTP integration
└── agent/            # Phase 5: Intent & Planning Layer
    ├── intent.rs     # TaskIntent, Goal, ConstraintSet, TaskIntentParser
    ├── analysis.rs   # EvidenceAnalyzer, TaskAnalysis, content grouping/anomalies
    ├── recommendation.rs # RecommendationEngine, Recommendation, ProposedOperation
    ├── clarification.rs # ClarificationEngine, UserDecision, DecisionCategory
    ├── plan.rs       # PlanGenerator → OperationPlan, FileSystemOperation
    ├── validate.rs   # PlanValidator → ValidationResult, PlanPreview
    └── executor.rs   # Executor → ApplyResult, OperationLog, UndoResult
```

### 1.2 Data Flow

**Correct data flow**: `TaskIntent → TaskAnalysis → Recommendation → OperationPlan → ValidationResult → ApplyResult(OperationLog) → UndoResult`

- **5A→5B**: `TaskIntentParser.parse()` → `TaskIntent`
- **5B**: `EvidenceAnalyzer.analyze(&intent)` → `TaskAnalysis` (scans filesystem, runs classification)
- **5C**: `RecommendationEngine.recommend(&intent, &analysis)` → `Recommendation`
- **5E**: `PlanGenerator.generate(&recommendation, &analysis, &[])` → `OperationPlan`
- **5F**: `PlanValidator.validate(&plan, &intent)` → `ValidationResult`; `PlanPreview.render()` → `String`
- **5G**: `Executor.execute(&plan, &validation, false)` → `ApplyResult` (contains `OperationLog`)
- **Undo**: `Executor.undo(&log)` → `UndoResult`

**Key observation**: The `EvidenceAnalyzer` in `analysis.rs` calls `scanner.scan()` and `run_classification()` internally, tightly coupling scanning/classification with analysis. This means downstream consumers calling `analyze()` get a cached scan, but cannot inject pre-scanned evidence.

**Cross-subphase references**: `clarification.rs` imports from `intent.rs` and `recommendation.rs`; `analysis.rs` imports from `classification/` and `evidence.rs`; `plan.rs` imports from `analysis.rs` and `recommendation.rs`. The agent module has internal coupling that prevents independently testing/replacing sub-components.

### 1.3 AI Boundary

AI classification (`MockAiClassifier`, `OpenAiProvider`) is properly gated behind `cfg(feature = "network")` for OpenAI. The `RecommendationEngine` does NOT directly call AI providers — it consumes `ClassificationResult` from `TaskAnalysis`. This is correct.

However, the CLI's `classify-ai` command (`cli.rs:244-302`) directly instantiates `RealAiClassifier::OpenAi(...)` and `MockAiClassifier`, which is correct for CLI but means AI provider wiring lives in the binary, not the library.

### 1.4 Binary vs Library Coupling

The binary (`main.rs` → `cli.rs`) directly imports internal types: `Scanner`, `ClassificationProcessor`, `RuleBasedProcessor`, `OpenAiProvider`, etc. The CLI is the only orchestration layer. Third-party Rust libraries consuming `folder-intelligence` as a dependency must use the flat re-exports from `lib.rs`.

**Problem**: There is no high-level "run this intent" API. Every consumer must chain `TaskIntentParser → EvidenceAnalyzer → RecommendationEngine → PlanGenerator → PlanValidator → Executor` themselves, exactly as the CLI does (`cli.rs:359-393`). This pipeline orchestration logic is duplicated in the CLI and would be duplicated by any future MCP adapter or programmatic user.

### 1.5 Concurrency

`Cargo.toml` declares `rayon = "1.10"` and `walkdir = "2.5"` and `bstr = "1.10"` as dependencies, but **none are imported anywhere in the source code**. The scanner uses `std::fs::read_dir` (sequential). The project is fully sequential — no parallelism despite the dependencies being declared.

### 1.6 Schema Versions

- Evidence schema: `SCHEMA_VERSION = "2.0.0"` (evidence.rs:5)
- Classification result schema: `"3.0.0"` (decision.rs:67, decision.rs:212)
- Schema files use different JSON Schema draft versions:
  - `directory-evidence.json`: draft-07
  - `scan-metadata.json`: draft-07
  - `classification-result.json`: 2020-12

### 1.7 Test Coverage

- Inline tests in `scanner.rs` (1494 lines, ~40% test code)
- Separate test files in `src/agent/` for each module: `intent_tests.rs`, `analysis_tests.rs`, `recommendation_tests.rs`, `clarification_tests.rs`, `plan_tests.rs`, `validate_tests.rs`, `executor_tests.rs`
- Integration tests in `tests/`: `classifier_ai_tests.rs` (9), `classifier_tests.rs` (11), `integration_tests.rs` (11), `unit_tests.rs` (26)
- Total: 170 lib tests + 57 integration tests = 227 tests
- No tests for `cli.rs` — CLI parsing and orchestration are untested
- No tests for `openai_provider.rs` — OpenAI integration has zero test coverage (only mock AI is tested)

## 2. Boundary Violations

### 2.1 CLI Intertwined with Core

`cli.rs` imports core types directly and performs full pipeline orchestration inline for each command. For example, the `validate` command (`cli.rs:375-393`) manually chains: `TaskIntentParser → EvidenceAnalyzer → RecommendationEngine → PlanGenerator → PlanValidator → PlanPreview`. This is correct orchestration logic that would need to be duplicated by any MCP adapter.

**Severity**: Medium — not a data flow violation, but creates coupling where CLI concerns (arg parsing, JSON output) are mixed with pipeline orchestration.

### 2.2 EvidenceAnalyzer Tightly Coupled to Scanner

`EvidenceAnalyzer::analyze()` (`analysis.rs:142-188`) internally creates a `Scanner` and calls `scan()`. This means:
- Cannot inject pre-cached scan results
- Cannot reuse a single scan across multiple analyses
- Any consumer calling `analyze()` triggers a full filesystem scan

**Severity**: Medium — limits flexibility for programmatic use and testing.

### 2.3 Recommendation Engine Not Trait-Bound

`RecommendationEngine` is a concrete struct, not a trait. There is no `RecommendationProvider` trait that could swap in AI-based reasoning vs deterministic reasoning. The engine is deterministic, but the design doesn't allow for alternative implementations.

**Severity**: Low — current implementation is correct, but limits future extensibility.

### 2.4 Dead Dependencies Declare Intent

`Cargo.toml` declares `walkdir`, `rayon`, and `bstr` but none are used. This creates confusion about intended capabilities (parallel scanning, walkdir-based traversal, binary string processing).

**Severity**: Low — cleanup opportunity.

### 2.5 `Default` on Unit Structs

All engine structs (`EvidenceAnalyzer`, `RecommendationEngine`, `PlanGenerator`, `PlanValidator`, `ClarificationEngine`, `PlanPreview`, `Executor`) implement `Default` but are unit structs. Clippy warns about `Foo::default()` vs `Foo`. This is a style issue, not a boundary violation.

**Severity**: Low — style.

## 3. Public API Candidates

### 3.1 Current `lib.rs` Exports

The library currently exports ALL public types from a flat namespace. This makes the API surface enormous and unstructured. There is no distinction between "core public API" and "internal implementation details".

**Candidate stable modules for public API**:

| Module | Purpose | Current Export |
|--------|---------|----------------|
| `evidence` | Filesystem data model | Partially (DirectoryEvidence, ScanLimits, etc.) |
| `scanner` | Directory scanning | `Scanner` |
| `classification` | File categorization | All types |
| `agent::intent` | Intent parsing | `TaskIntent`, `Goal`, `ConstraintSet` — but `TaskIntentParser` not in lib.rs! |
| `agent::analysis` | Evidence analysis | `EvidenceAnalyzer`, `TaskAnalysis` |
| `agent::recommendation` | Recommendation engine | `RecommendationEngine`, `Recommendation` |
| `agent::clarification` | User interaction | `ClarificationEngine` |
| `agent::plan` | Plan generation | `PlanGenerator`, `OperationPlan`, `FileSystemOperation` |
| `agent::validate` | Validation | `PlanValidator`, `ValidationResult` |
| `agent::executor` | Execution & undo | `Executor`, `ApplyResult`, `OperationLog` |

### 3.2 Missing Export: `TaskIntentParser`

`TaskIntentParser` is defined in `intent.rs` but is NOT exported in `lib.rs`. It is only accessible via `crate::agent::TaskIntentParser` within the binary. This means external library consumers cannot parse user intent — a critical gap.

### 3.3 Missing High-Level Pipeline API

There is no high-level `run_pipeline(intent: &TaskIntent) -> ApplyResult` function. Every consumer (CLI, future MCP, tests) must manually chain 5+ components.

### 3.4 Serialization as Public Contract

All key types implement `Serialize`/`Deserialize`. The `OperationLog`, `ApplyResult`, `UndoResult`, `OperationPlan`, `ValidationResult`, `TaskIntent`, `TaskAnalysis`, `Recommendation` are all serializable. This is correct for persistence/recovery, but there are no schema files for these Phase 5 types — only for Phase 0–2 evidence and classification results.

## 4. Dead/Duplicated Code

### 4.1 Dead Dependencies

| Dependency | Declared | Used | Status |
|------------|----------|------|--------|
| `walkdir` | Cargo.toml:14 | No | Dead |
| `rayon` | Cargo.toml:16 | No | Dead |
| `bstr` | Cargo.toml:17 | No | Dead |

### 4.2 Dead Code Annotations

Several structs and methods have `#[allow(dead_code)]`:
- `FileSystemOperation::operation_type()` (plan.rs:28-34) — used in executor
- `FileSystemOperation::source_path()` / `dest_path()` (plan.rs:52-66) — used in plan.rs
- `FileSystemOperation::description()` (plan.rs:37-50) — used in validate.rs preview
- `DecisionCategory` and all its methods (clarification.rs:11-48) — used in tests only
- `ValidationStatus::is_conflict()` (validate.rs:39) — unused
- `PlanValidator` methods: `validate_plan_consistency`, `filter_executable` — unused
- `PlanGenerator::preview` (plan.rs:452-508) — replaces `PlanPreview` in validate.rs?

### 4.3 Potentially Duplicated Logic

- **Path containment checking**: `is_within_scope` is duplicated in both `plan.rs` (PlanValidator) and `plan.rs` (PlanGenerator), as `validate_operations` in plan.rs.
- **Content type grouping**: `ContentType` enum exists in both `analysis.rs` (agent) and `classification/result.rs` (classification). They are different types with similar purposes.
- **Time utilities**: `now_secs()` pattern (SystemTime → UNIX_EPOCH → as_secs) is repeated in `intent.rs`, `analysis.rs`, `recommendation.rs`, `plan.rs`, `validate.rs`, and `executor.rs`. No shared utility module.

### 4.4 Dead Code: `Recommendation` Fields

The PHASE5.md doc describes `Recommendation` with fields `risks: Vec<Risk>`, `alternatives: Vec<AlternativePlan>`, but the actual `Recommendation` struct (`recommendation.rs:120-132`) has no such fields. The doc is ahead of the implementation.

### 4.5 Dead Code: `FileSystemOperation` Operations

PHASE5.md describes `FileSystemOperation` with `Copy` and `Rename` variants, but the actual enum (`plan.rs:13-25`) only has `Move`, `CreateDir`, `Delete`. The doc is ahead of the implementation.

## 5. Documentation Inconsistencies

### 5.1 PHASE5.md vs Implementation Mismatch

| Documented | Actual |
|-----------|--------|
| `Recommendation.risks: Vec<Risk>` | Not present |
| `Recommendation.alternatives: Vec<AlternativePlan>` | Not present |
| `FileSystemOperation::Copy { source, dest }` | Not implemented |
| `FileSystemOperation::Rename { from, to }` | Not implemented |
| `OperationLog.undo_script: UndoScript` | Not implemented (replaced by deterministic UndoResult) |

### 5.2 Schema Draft Version Mismatch

`classification-result.json` uses JSON Schema draft 2020-12 while `directory-evidence.json` and `scan-metadata.json` use draft-07. Inconsistent schema standards within the same project.

### 5.3 No Schema for Phase 5 Types

There are JSON schemas for evidence (v2.0.0) and classification results (v3.0.0), but no schemas for:
- `TaskIntent`
- `TaskAnalysis`
- `Recommendation`
- `OperationPlan`
- `ValidationResult`
- `OperationLog` / `ApplyResult`
- `UndoResult`

These are all `Serialize`/`Deserialize` but have no formal schema documentation, making it harder for external consumers to validate JSON I/O.

### 5.4 CLI Help/Documentation Gap

There is no `--help` output generated by the CLI. Users must read the code to know available commands and flags. No `README.md` at the project root.

## 6. Recommended Phase 6 Architecture

### 6.1 Layered Architecture

```
┌─────────────────────────────────────────────────┐
│           Adapters (Consumers)                  │
│  CLI binary  │  MCP server  │  Library API  │  HTTP API  │  GUI  │
├─────────────────────────────────────────────────┤
│               Core Library API (6A)             │
│                                                 │
│  folder_intelligence::                        │
│  ├── scanner/        (Phase 0-2)              │
│  ├── evidence/                                │
│  ├── classification/  (Phase 3-4B)                            │
│  └── agent/          (Phase 5)                 │
│      ├── intent/                               │
│      ├── analysis/                             │
│      ├── recommendation/                       │
│      ├── clarification/                        │
│      ├── plan/                                 │
│      ├── validate/                             │
│      └── executor/                             │
└─────────────────────────────────────────────────┘
```

### 6.2 Phase 6 Definition

**Phase 6 = Integration & Interface Layer**

This is NOT about adding more AI capability. It's about making the Phase 5 core consumable by external systems.

**6A — Core Library API (Priority: High)**
- Clean, documented public API surface via `lib.rs`
- High-level pipeline function: `run_organize_intent(request: &str, scope: &Path) -> Result<ApplyResult, Error>`
- Export `TaskIntentParser` (currently missing)
- Module-level documentation with examples
- JSON schema files for all Phase 5 serializable types

**6B — MCP Adapter (Priority: Medium)**
- Separate crate/binary: `folder-intelligence-mcp`
- Only depends on `folder-intelligence` library crates
- Exposes tools: `scan`, `classify`, `intent`, `analyze`, `recommend`, `clarify`, `plan`, `validate`, `apply`, `undo`

**6C — Library API Documentation (Priority: High)**
- Generate rustdoc with full API documentation
- Usage examples for common workflows

**6D — Provider Expansion (Priority: Low)**
- Only after core API is stable
- Add Claude provider, Gemini provider, etc. as optional features

### 6.3 Decoupling Requirements

For MCP/adapters to work cleanly:
1. All filesystem scanning should be injectable into `EvidenceAnalyzer` (allow passing `ScanResult`)
2. Pipeline orchestration should be a library function, not CLI-specific
3. `TaskIntentParser` must be public
4. AI provider selection should be a parameter, not CLI-hardcoded

## 7. Required Changes Before Phase 6

### 7.1 Immediate (Before 6A)

1. **Export `TaskIntentParser` in `lib.rs`** — currently missing, blocks programmatic intent parsing
2. **Add high-level pipeline API** to `agent/mod.rs` or a new `agent/pipeline.rs`:
   ```rust
   pub fn run_pipeline(
       intent: &TaskIntent,
       force: bool,
   ) -> Result<(OperationPlan, ValidationResult, ApplyResult), Error>
   ```
3. **Remove dead dependencies** from `Cargo.toml`: `walkdir`, `rayon`, `bstr`
4. **Fix clippy warnings**: Change `::default()` to direct construction for unit structs
5. **Fix `#[allow(unused_mut)]`** in `validate_tests.rs:178`

### 7.2 Before 6B (MCP)

6. **Add `allow_inject_scan()` to `EvidenceAnalyzer`** or create a constructor that accepts pre-scanned `ScanResult`
7. **Create JSON schemas** for all Phase 5 types (`TaskIntent`, `OperationPlan`, `OperationLog`, etc.)
8. **Split recommendation doc mismatches**: either implement `risks`/`alternatives` or update PHASE5.md

### 7.3 Not Needed Before Phase 6

- New AI providers (6D can come later)
- GUI implementation
- HTTP API server
- Additional FileSystemOperation variants (`Copy`, `Rename`) — not blocking
- Parallel scanning (rayon) — optimization, not architectural requirement

## Summary

Phase 5 provides a complete execution loop with proper authority separation (AI suggests, deterministic layer executes). The core is sound but needs API cleanup before external consumption. The top priorities are: export `TaskIntentParser`, add a high-level pipeline function, remove dead dependencies, and fix documentation mismatches. MCP should come AFTER these API cleanups, not before.
