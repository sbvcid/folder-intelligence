# folder-intelligence Design Specification

Status: Draft v0.1

## 1. Purpose

`folder-intelligence` is a filesystem intelligence layer. Its first responsibility is to observe a filesystem and produce compact, structured evidence that downstream software, AI agents, search systems, and applications can interpret.

The project is not an opinionated folder organizer and does not embed a fixed taxonomy. The user's filesystem remains the source of truth.

Core principle:

> Filesystem is the source of truth. Evidence is observation. Interpretation belongs downstream. Actions require validation.

## 2. Core Boundaries

### Scanner

The Rust scanner observes filesystem state. It may collect paths, names, counts, sizes, extensions, directory structure, representative filenames, notable files, syntactic identifiers, timestamps, and other bounded evidence.

The scanner MUST NOT decide what a directory means. It MUST NOT classify content as music, books, games, documents, etc. based on hard-coded semantic rules.

### Evidence

Evidence records observations and their provenance. Evidence should preserve useful raw signals rather than prematurely converting them into semantic labels.

Evidence is designed to be:

- deterministic where possible
- bounded in resource use
- streamable
- machine-readable
- independent of any AI provider
- useful to both humans and software

### Interpretation

AI or another downstream application interprets evidence. Interpretation may include classification, grouping, similarity, duplicate hypotheses, summaries, and uncertainty.

Interpretation is not filesystem truth and must not overwrite the underlying evidence.

### Planning and Actions

AI produces proposed decisions or plans. A separate validation/execution layer is responsible for checking and applying filesystem changes.

AI MUST NOT directly mutate the filesystem.

## 3. System Architecture

```text
Filesystem
    |
    v
Scanner / Evidence Acquisition
    |
    v
Evidence
    |
    +-------------------+
    |                   |
    v                   v
Index / Search     Progressive Evidence
    |                   |
    +---------+---------+
              |
              v
       AI / Agent Interpretation
              |
              v
        Classification
              |
              v
             Plan
              |
              v
          Validation
              |
              v
           Preview
              |
              v
            Apply
              |
              v
        Operation Log
              |
              v
             Undo
```

The first implementation only needs the Scanner/Evidence layer. Later layers must be added without making them mandatory dependencies of the core scanner.

## 4. Evidence Model

Evidence should be acquired progressively according to information value and cost.

### Level 0: Cheap structural evidence

Examples:

- absolute path
- directory name
- parent path
- direct file count
- direct child-directory count
- direct file-size total
- extension histogram
- child-directory names
- timestamps when useful

### Level 1: Cheap contextual evidence

Examples:

- representative filenames
- notable filenames
- filename patterns
- syntactic identifiers
- README/NFO/TXT/MD presence
- bounded text previews

### Level 2: Conditional evidence

Collected only when it can reduce uncertainty:

- archive entry listings
- media metadata
- image dimensions
- audio/video duration and codec
- additional bounded text

### Level 3: Expensive evidence

Not collected by default:

- OCR
- VLM/image interpretation
- audio/video semantic analysis
- full-content hashing when unnecessary
- expensive deep inspection

The implementation should follow an Evidence Budget: maximize information gained per unit of CPU, memory, I/O, and model context.

## 5. Evidence Is Not Semantics

Identifier detection is syntactic. For example, detecting an alphanumeric code or ISBN-like string does not establish what the directory contains.

Similarly, `.mp3` is evidence that MP3 files exist; it is not a hard-coded declaration that the directory is an audio category.

This distinction is fundamental because the same evidence layer must work across arbitrary domains and user-defined folder structures.

## 6. Classification by Filesystem Precedent

The primary classification model is not simply:

```text
folder name -> semantic category
```

Instead, the system should compare a target directory against the user's existing filesystem organization.

A classification candidate can contain:

- category path
- category-level evidence
- representative existing items
- common extension patterns
- common filename patterns
- common directory structure
- identifier patterns
- other observed fingerprints

The AI then evaluates how closely the target resembles the existing precedent.

Example:

```text
Unclassified/A
  51 mp3
  3 jpg
  README.txt
  cover.jpg
  RJ01547914

Existing category:
Audio/Japanese/
  many existing folders
  mostly mp3
  similar README/cover structure
  similar identifier patterns
```

The category is strong evidence because the filesystem already demonstrates how similar material is organized there.

The project therefore treats existing filesystem structure as a source of classification examples rather than requiring a globally predefined taxonomy.

## 7. Candidate Categories

Candidate discovery should happen before expensive AI reasoning at scale.

For a target directory, the system should retrieve a bounded set of plausible destinations using cheap evidence and indexes. The AI should then compare the target with those candidates.

A candidate may be:

1. an existing populated category with similar precedents;
2. an existing category with weak or ambiguous similarity;
3. an empty category;
4. a proposed new category that does not currently exist.

An empty directory is evidence of an intended category only to the extent supported by its path, parent context, and other filesystem evidence. It must not automatically outrank populated categories.

## 8. Classification Decisions

The classification layer should support at least these outcomes:

```text
MOVE_EXISTING
CREATE_CATEGORY
LEAVE_UNCLASSIFIED
ASK_USER
```

`MOVE_EXISTING` means an existing destination is sufficiently supported by evidence.

`CREATE_CATEGORY` means no existing destination is sufficiently suitable and the evidence supports a new category proposal.

`LEAVE_UNCLASSIFIED` means available evidence is insufficient and automatic action is not justified.

`ASK_USER` is appropriate when multiple alternatives remain materially ambiguous.

Classification should include confidence, alternatives, uncertainty, and evidence references rather than only a destination string.

## 9. Interpretation Output

A future interpretation record should conceptually contain:

```json
{
  "source": "path/to/target",
  "decision": {
    "type": "MOVE_EXISTING",
    "destination": "path/to/category",
    "confidence": 0.91
  },
  "alternatives": [],
  "evidence": [],
  "uncertainty": [],
  "model": "provider/model",
  "created_at": 0
}
```

The exact schema is intentionally deferred until the evidence prototype has been benchmarked.

## 10. Similarity and Fingerprints

The system should eventually support several kinds of similarity without assuming they are equivalent:

- exact identity
- near-duplicate identity
- structural similarity
- filename-pattern similarity
- identifier similarity
- media-property similarity
- semantic similarity

A folder fingerprint can summarize useful structural signals such as extension distribution, file-count characteristics, directory structure, filename patterns, text-file presence, and identifier patterns.

Fingerprints are retrieval aids, not semantic truth.

## 11. Progressive Evidence

The system should eventually support an evidence loop:

```text
cheap evidence
    |
    v
AI interpretation
    |
    +---- sufficient confidence ---> conclusion
    |
    +---- uncertainty -------------> targeted evidence request
                                         |
                                         v
                                  scanner acquires evidence
                                         |
                                         v
                                  AI re-evaluates
```

This prevents expensive inspection of every file while allowing difficult cases to receive additional evidence.

## 12. Safety and Filesystem Mutation

Classification and filesystem modification are separate concerns.

The required action pipeline is:

```text
Classification
    -> Plan
    -> Validate
    -> Preview
    -> Apply
    -> Operation Log
    -> Undo
```

The executor must validate paths and filesystem state independently of the AI output.

An undo operation must verify that the filesystem has not changed in a way that makes the rollback unsafe. It must refuse unsafe rollback rather than blindly reversing an old command.

Plans and operation records should be preserved separately from evidence so multiple AI models can be compared on the same original filesystem state.

## 13. Multi-Model Evaluation

The core evidence must be provider-neutral.

The same evidence batch should be usable with different models, for example local models and hosted APIs. Model outputs should be stored separately so results can be compared without rescanning the filesystem.

A future benchmark should evaluate:

- classification accuracy
- correct existing-category selection
- correct new-category decisions
- false-positive moves
- false-negative moves
- uncertainty calibration
- evidence grounding
- consistency across models
- cost and latency

## 14. Indexing

SQLite or another index may be introduced later as a rebuildable cache/index, not as the source of truth.

The conceptual separation is:

```text
Filesystem = source of truth
Evidence   = observed snapshot
Index      = rebuildable acceleration layer
AI result  = interpretation
Plan       = proposed action
Log        = history of applied actions
```

The index should make it possible to retrieve candidate categories without sending the whole filesystem to an LLM.

## 15. External Interfaces

The core should remain independent of any specific AI client or UI.

Potential future interfaces include:

- CLI
- library/API
- MCP adapter
- OpenAI-compatible providers
- Anthropic
- Gemini
- Bedrock
- OpenRouter
- Ollama/local models
- GUI applications
- third-party services

MCP and GUI layers should wrap the core rather than become core dependencies.

## 16. Current Implementation Constraints

The current repository is an early Rust prototype with a Scanner, evidence types, JSONL CLI output, JSON Schema, and integration/unit tests. The existing design already establishes the core observation-vs-interpretation boundary.

Before adding AI, MCP, SQLite, or filesystem actions, the scanner contract must be corrected and tested.

Known areas requiring verification or correction include:

- actual enforcement of `max_depth`
- global `max_total_files` enforcement
- global `max_total_dirs` semantics
- `max_files_per_dir` semantics
- accurate definition of `total_size`
- representative-file sampling quality
- path normalization in `inspect`
- inspect efficiency
- scan error semantics
- identifier noise and deduplication
- Windows Unicode and filesystem edge cases
- limit/timeout test coverage

The README and schema must describe actual behavior rather than intended future behavior.

## 17. Non-Goals for the Current MVP

Do not add the following until the evidence layer and classification hypothesis have been validated:

- built-in LLM dependency
- mandatory AI provider
- MCP server
- SQLite requirement
- GUI
- automatic filesystem modification
- fixed domain-specific taxonomy
- expensive OCR/VLM/audio analysis by default
- embeddings as a mandatory dependency

## 18. Design Rule

When deciding whether to add a feature, prefer this question:

> Does this improve the ability to observe, retrieve, interpret, validate, or safely act on arbitrary filesystem state without making the filesystem itself cease to be the source of truth?

If not, the feature probably belongs outside the core.
