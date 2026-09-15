# Phase 4A — Provider-Agnostic AI Classification

## Status

In progress.

## Goal

Allow AI to consume Phase 2/3 evidence for semantic classification, **without AI touching the filesystem**.

AI can reinterpret evidence but cannot fabricate or modify evidence.

## Architecture

```text
Filesystem
    ↓
Scanner
    ↓
DirectoryEvidence
    ↓
RuleBasedProcessor
    │   ├── deterministic classification (baseline)
    │   └── safety signal (confidence, warnings)
    ↓
ClassificationInput
    ↓
AI Processor
    │   ├── consumes evidence + rule-based result
    │   ├── produces enhanced ClassificationResult
    │   └── cannot modify evidence or filesystem
    ↓
ClassificationResult
    ↓
Phase 5: Plan → Validate → Preview → Apply
```

## Core Principles

### AI Sees
- `DirectoryEvidence` (target + candidate summaries)
- `ClassificationInput` (target, candidates, metadata)
- Rule-based `ClassificationResult` (baseline decision, scores, warnings)
- Confidence and uncertainty from rule-based engine

### AI Does NOT See
- Arbitrary filesystem access (no path traversal beyond evidence)
- Raw file contents (no reading file data)
- OS commands
- Filesystem mutation (no move/copy/delete)
- Credentials or tokens

## Trait Interface

```rust
pub trait AiClassifier {
    fn classify(
        &self,
        input: &ClassificationInput,
        baseline: &ClassificationResult,
    ) -> Result<ClassificationResult, AiClassificationError>;
}
```

The `baseline` parameter gives the AI model the rule-based result as context, allowing it to:
- Confirm or override deterministic decisions
- Provide semantic interpretation (e.g., "this looks like a music collection")
- Add uncertainty reasons
- Adjust confidence within bounds

## Error Model

```rust
pub enum AiClassificationError {
    ProviderError(String),        // API/network error
    InvalidResponse(String),      // Malformed JSON or missing fields
    Timeout,                      // Request timed out
    Unauthorized,                 // Auth failure
    RateLimited,                  // Rate limit exceeded
}
```

## Structured Prompt

The AI receives a JSON-serialized `AiClassificationRequest`:

```json
{
  "target_evidence": { ... DirectoryEvidence ... },
  "candidates": [ ... CandidateEvidence ... ],
  "rule_based_result": { ... ClassificationResult ... },
  "constraints": {
    "max_confidence_delta": 0.2,
    "allowed_decisions": ["move_existing", "create_category", "leave_unclassified", "ask_user"],
    "force_user_hints": null
  }
}
```

### Constraints
- `max_confidence_delta`: AI confidence cannot exceed rule-based confidence by more than this delta (prevents overconfidence)
- `allowed_decisions`: AI can only produce decisions from this set
- User hints (e.g., "force categorize as X") can override if provided

## Output Validation

AI response is JSON and must match `ClassificationResult` schema. Validation:
1. Deserialize into `ClassificationResult`
2. Check `decision` is in `allowed_decisions`
3. Check `confidence` is within `[baseline.confidence - max_confidence_delta, baseline.confidence + max_confidence_delta]`
4. Check all paths are valid
5. Reject if `selected_candidate` is set but decision is not `MoveExisting`

## Mock Provider (Phase 4A)

A deterministic `MockAiClassifier` that:
- Simulates AI classification by applying enhanced rules
- Produces predictable output for testing
- Never makes network calls
- Can be configured to simulate different AI behaviors

## CLI

```bash
fi classify-ai --target <path> --category-root <path> --model <model-identifier>
```

For Phase 4A, `--model mock` uses the `MockAiClassifier`.

## What NOT to implement in Phase 4A

- Actual provider implementations (OpenAI, Anthropic, Gemini, Bedrock, Ollama)
- MCP server/adapter
- Embeddings or vector database
- Filesystem mutation
- Prompt engineering frameworks beyond structured JSON
- Cost/latency tracking
- Operation log, planning, validation, preview, apply, undo
- Real API key management

## Definition of Done

```text
cargo test                              PASS
cargo clippy -- -D warnings            PASS
cargo build --release                  PASS

AiClassifier trait exists               PASS
AiClassificationRequest/Response model  PASS
Structured prompt builder              PASS
JSON-only output validation            PASS
MockAiClassifier deterministic         PASS
CLI classify-ai command                PASS
AI result → ClassificationResult       PASS
No filesystem mutation                 PASS
No provider implementations            PASS
No MCP                                PASS
No embeddings                         PASS
```

---

## Phase 4B — OpenAI-Compatible Provider Adapter

### Goals

1. First real provider implementation to validate `AiClassifier` abstraction
2. Prove core types are not polluted with provider-specific fields
3. Handle AI unreliability (malformed JSON, missing fields, hallucinated candidates)
4. No filesystem mutation

### Provider Architecture

```text
Core
├── AiClassifier (trait)
├── AiClassificationRequest / Response
├── ClassificationResult
└── validate_ai_result()

Providers
├── openai_provider.rs
│   ├── OpenAiProvider (implements AiClassifier)
│   └── OpenAiConfig { base_url, api_key, model, timeout }
└── (future: anthropic_provider.rs, gemini_provider.rs, etc.)
```

### Provider Config

```rust
pub struct OpenAiProviderConfig {
    pub base_url: String,        // e.g. "https://api.openai.com/v1"
    pub api_key: String,
    pub model: String,           // e.g. "gpt-4o"
    pub timeout: Duration,
}
```

### Prompt Structure

The provider sends `AiClassificationRequest` as structured JSON in the user message:

```
Classify the target directory evidence above against the provided candidates.

Use the rule-based baseline decision as a reference. You may confirm, refine, or override it.

Constraints:
- max_confidence_delta: 0.2 (your confidence must be within 0.2 of the baseline)
- allowed_decisions: [move_existing, create_category, leave_unclassified, ask_user]
- selected_candidate must be an existing candidate path (no hallucinated paths)
- If you cannot confidently decide, use leave_unclassified or ask_user

Output format: JSON matching ClassificationResult schema.
```

### Hallucination Prevention

AI must NOT invent candidate paths. The validation enforces:

1. If `selected_candidate` is set → it must match one of the input candidate paths
2. If `proposed_category_name` is set → it must be a known candidate name or the target name
3. All `alternatives.candidate_path` must exist in input candidates

### Error Handling

```text
valid JSON response
    → validate_ai_result()
    → ClassificationResult

malformed JSON
    → AiClassificationError::InvalidResponse

missing required field (deserialization error)
    → AiClassificationError::InvalidResponse

invalid enum variant
    → AiClassificationError::InvalidResponse

hallucinated candidate path
    → AiClassificationError::InvalidResponse

timeout (network)
    → AiClassificationError::Timeout

401/403
    → AiClassificationError::Unauthorized

429
    → AiClassificationError::RateLimited

other 4xx/5xx
    → AiClassificationError::ProviderError
```

### What NOT to implement in Phase 4B

- Multiple providers (only OpenAI-compatible in 4B)
- MCP server/adapter
- Embeddings or vector database
- Filesystem mutation
- Cost/latency tracking
- Operation log, planning, validation, preview, apply, undo
- Prompt engineering frameworks beyond structured JSON input

### Phase 4B Status

- `OpenAiProvider` + `OpenAiProviderConfig` implemented (src/classification/openai_provider.rs)
- Implements `AiClassifier` trait, OpenAI Chat Completions API with JSON schema response_format
- Hallucination prevention validates `selected_candidate`, `alternatives`, `proposed_category_name`
- Error mapping: timeout/401/403/429/provider errors
- `RealAiClassifier` enum dispatches Mock or OpenAi provider
- CLI `--api-key`, `--base-url` flags for `classify-ai`
- `network` cargo feature + `reqwest` (blocking) dependency
- 115 tests pass, clippy clean, release build succeeds

---

## Phase 5 — Intent & Planning Layer

### Revised Vision

Phase 5 is **not** a filesystem operations phase. It is an **intent-driven planning layer** that sits above the filesystem intelligence substrate (Phases 0–4B).

```
User (natural language)
    ↓
Task Intent
    ↓
Evidence Analysis (Phases 0–4B substrate)
    ↓
AI Reasoning & Recommendation
    ↓
User Interaction (clarify / explain)
    ↓
Operation Plan
    ↓
Validate → Preview → User Approval → Apply / Undo
```

The user does **not** say "classify into categories." The user says things like:

- "Help me organize Downloads."
- "Organize by 'Work / Personal / Entertainment'."
- "Reorganize my project folders."

### Phase 5 Sub-phases

| Sub-phase | Description | Deterministic / AI / User |
|-----------|-------------|---------------------------|
| **5A** | Natural-language intent parsing | AI + Deterministic |
| **5B** | Evidence-driven analysis | Deterministic (Phases 0–4B) |
| **5C** | AI reasoning & recommendation | AI |
| **5D** | Interactive clarification | User |
| **5E** | Operation plan generation | AI |
| **5F** | Validation & preview | Deterministic |
| **5G** | Apply / Undo | User approval + Deterministic |

### Key Principles

1. **AI is the orchestration/reasoning layer**, not a simple classifier
2. **ClassificationResult (Phase 3/4B)** is a tool within the AI reasoning pipeline, not an end product
3. **Multiple analysis modes**: classification, structure analysis, duplicate detection, orphan detection, naming inconsistency
4. **Explainability**: AI must explain its recommendations before proposing actions
5. **User control**: No filesystem mutation without explicit user approval
6. **Safe undo**: Every applied plan must be reversible
