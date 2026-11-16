# F02/F03: remove competing document coordinates from generative chunks

Historical F02/F03 drafts copied `chunks.start/end` (document-global) into evidence ranges, although those ranges are chunk-relative. Merely strengthening the suffix did not establish a recovery (root's separate suffix comparison0→0). The context exposed two coordinate systems using the same field names: full-document `chunks.start/end`, and local `quote_ranges.start/end`.

The production context now omits only `start/end` in serialized chunk objects. It retains id/source_id/document_version/field/text and all relative quote_ranges. Internal Chunk bounds, original source text/version/hash and returned public witness offsets are untouched. No new models, schemas, allowances or dependencies. Audit's resolved exact witness still includes global source offsets; those belong to returned evidence, not a generative range selection request.

Regression `generative_chunks_omit_document_offsets_but_witnesses_keep_them` uses both frozen F02/F03 contexts. It compares every remaining serialized field with the original, verifies internal global bounds survive and resolves chunk-relative selected starts to original Unicode witness ranges/version. Original serialization fails the same test; new serialization passes. Offline full suite159unit+6stdio, fmt and clippy pass. No model/network calls by this agent; retrieval/answer gains remain unproven.

## Controlled model component

Existing ignored `coverage_live_saved_range_contexts` now additionally accepts test-only `coordinates_baseline` / `coordinates_candidate`. Both use the **same new** production RFC_RANGE_PROMPT, same RULES/DRAFT_PROMPT, exact source text and unchanged frozen fixture. Baseline keeps original context; candidate removes only document-global start/end from chunks. Original document SHA256/URL/chunk-slice checks remain mandatory.

Coordinate arms make exactly one draft call per case (F02,F03) and skip audit, avoiding an evaluator confound and extra calls. They preserve inputs/raw draft/selected/pruned results and failure without retry. The final session result is unaudited and is not a semantic quality verdict; compare raw generated ranges and admitted witnesses in selected/pruned artifacts. Old suffix arms remain available; old fixtures/results were not rewritten. Use fresh output directories.

```sh
RESEARCH_COVERAGE_CORPUS_DIR="$PWD/runs/qwen-acceptance-2026-11-13/candidate/new" RESEARCH_COVERAGE_RANGE_ARM=coordinates_candidate RESEARCH_GATE_REPLAY_DIR="$PWD/outputs/coverage-coordinates-candidate" CARGO_HOME="$PWD/.cache/cargo" cargo test --locked --offline coverage_live_saved_range_contexts -- --ignored --exact research::tests::coverage_live_saved_range_contexts
```

Baseline changes only arm and output directory. Root serializes model access, measures outcomes and retains the earlier failed suffix experiment separately. Scope is a structural context fix, not acceptance or release.
