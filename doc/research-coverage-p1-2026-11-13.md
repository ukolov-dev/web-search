# Coverage P1: RFC clause boundary and range guidance

G02 component reproduced and fixed: the complete original sentence `The names within an object SHOULD be unique.` was rejected because the next **separate paragraph** starts with lowercase ABNF `object = ...`. The old lowercase-continuation heuristic ignored the intervening blank line. The fix distinguishes a new paragraph from an inline or single-newline continuation; it preserves exact source coordinates, complete literal clause, RFC identity and modality gates.

Baseline35765fe; Task TASK-20261113-144125-001. Scope: src/research.rs RFC admission and RFC draft-range guidance, one focused test in src/research_tests.rs. No changes to retrieval, safe_claim_text, repair budget, schema, dependencies, Qwen/news selection or public interfaces. Parent owns Router lifecycle and independent acceptance.

## Historical stage diagnosis

| Case | Observed first break | Outcome here |
| --- | --- | --- |
| G02 | Required SHOULD literal exists in S1P2, draft selects exact873..917 range; admission rejects because following ABNF paragraph is lowercase | Deterministic red→green regression; supported required claim retained after audit, one body still insufficient for final sufficient gate |
| G08 | Selected context/draft describe VecDeque::insert while question asks Vec::insert | Audit correctly rejects; handed to documentation agent |
| F02 | Generated ranges use document-level79000..80200 etc and whole1200-character chunks instead of chunk-relative12..360 | Existing selection gate correctly rejects; clarify guidance, model replay remains to run |
| F03 | Same incorrect ranges, plus invalid/incomplete claim text | Guidance candidate only; gates unchanged |
| F12 | Definition/cache/other background fills draft; required429 obligation remains missing[0,1] | No unsupported correction. Initial reading of keyword optional was corrected: phrase belongs to428, not429. The selected-context support for429 optionality is not established |

Evidence uses candidate raw model plan/draft/audit and action logs in existing quote-binding-confirm and qwen-acceptance corpora. Cohorts remain separate; this is not a new paired result. Full diagnostic extraction retained locally under outputs/coverage-p1/stages.json; concise tracked evidence is [JSON](../feasibility/research-coverage-p1-2026-11-13.json).

The range instruction now explicitly states12..360 Unicode characters, bounds within named chunk, and forbids using document start/end offsets or whole1200-character chunks. Existing prompt already said chunk-relative; the additional explanation targets the observed repeated misinterpretation without loosening validation. Its impact on real model output is **unmeasured**, to be tested by root on exact saved contexts.

## Verification

- `CARGO_HOME="$PWD/.cache/cargo" cargo test --locked --offline rfc_complete_clause_before_lowercase_paragraph_keeps_required_fact`: baseline failed with RFC complete-clause assertion; fixed passed1. The fixture uses exact historical clause and adjacent paragraph pattern, not a new live search. Inline/single-newline lowercase continuations remain rejected; audit retains the claim without bypassing two-body sufficiency.
- `CARGO_HOME="$PWD/.cache/cargo" cargo test --locked --offline`: passed, counts in linked JSON; ignored network/model cases not run.
- `cargo fmt --check`: passed.
- `CARGO_HOME="$PWD/.cache/cargo" cargo clippy --locked --offline --all-targets -- -D warnings`: passed.

An initial regression-test harness attempted Draft.clone (unsupported); this compilation mistake was corrected and is not counted as defect reproduction. Actual red evidence was subsequently captured against the original production condition. Release build, local Qwen/model replay, fresh paired questions, Harness and independent reviewer are delegated to parent integration; this scoped implementation does not close the complete coverage backlog or erase historical FAILED gates.

## F02/F03 source-range component probe

New ignored `coverage_live_saved_range_contexts` reads exact frozen first-draft contexts from `tests/fixtures/research/coverage-range-contexts.json` and original document JSON from `RESEARCH_COVERAGE_CORPUS_DIR`. URL, SHA256 of original text and each chunk slice are checked before inference. Corpus paths are relative and reject traversal; full originals remain outside Git. Candidate uses production `RFC_RANGE_PROMPT`; old final-losses probe now also reuses that constant.

`RESEARCH_COVERAGE_RANGE_ARM=baseline|candidate` is **test-only**. This comparison isolates the range suffix: both arms share the checkout's RULES/DRAFT_PROMPT, source admission and audit code, so it is not a full historical-runtime baseline. Old suffix is frozen from the previous production instruction. Each case calls draft once and audit once only when admitted claims exist; failed/refused raw calls are saved with a failure marker and never retried. Outputs include inputs, raw draft, selected/pruned claims, audit and final result. A successful test exit means transport/processing completed, not sufficient answer or quality-gate pass; the root reviewer must inspect evidence.

Example command (root owns invocation and actual inference):

```sh
RESEARCH_COVERAGE_CORPUS_DIR="$PWD/runs/qwen-acceptance-2026-11-13/candidate/new" RESEARCH_COVERAGE_RANGE_ARM=candidate RESEARCH_GATE_REPLAY_DIR="$PWD/outputs/coverage-range-candidate" CARGO_HOME="$PWD/.cache/cargo" cargo test --locked --offline coverage_live_saved_range_contexts -- --ignored --exact research::tests::coverage_live_saved_range_contexts
```

Probe compilation checked with `CARGO_HOME="$PWD/.cache/cargo" cargo test --locked --offline --no-run`; source hashes/context slices checked offline. No model/network invocation by this agent. Initial probe compile errors (Document fields, save Value conversion) were corrected before handoff and do not constitute product evidence.
