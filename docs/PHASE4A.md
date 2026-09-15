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
