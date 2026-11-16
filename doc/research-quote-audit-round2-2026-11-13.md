# Round 2: quote-bound audit input

The previous round remains FAILED: independent review found a new unknown-as-cited G09/A2 despite aggregate gains. Its returned quote proves only `assert_eq!(vec.pop(), Some(3));`; initial vec![1,2,3] and remaining[1,2] occur elsewhere in the chunk. The saved real audit nevertheless supported the full statement by consulting those other lines. This is historical red evidence, not an invented deterministic model response.

## Change

`Session::audit_context` replaces every claim's evidence with resolved exact witnesses including source ID, document version and Unicode start/end, using existing quote validation. Full chunks are explicitly `rejection_context`: available only to reject/disambiguate, identify conflicts or target missing evidence. They cannot supply omitted support for a claim. The original question may supply explicitly given inputs for an inference; it cannot be mined for unstated assumptions. A single concrete example does not establish a general API contract.

The existing audit call and schema are reused. After resolution the entire input is bounded to24000 Unicode characters: only rejection chunks may be removed; question, claims and returned witnesses are never trimmed. Visible IDs include mandatory witness chunks and remaining rejection chunks, preserving exact conflict validation without pretending trimmed background was visible. Source text itself is never changed.

Audit guidance also requires kind=inference plus a substitution rationale when a documented general rule is applied to concrete inputs from the question. This distinguishes the V1 Harness H02 classification issue from a factually false/unknown answer. Correct classification and semantic support still depend on model review; no deterministic numeric whitelist is added.

## Tests and limits

- Actual G09 fixture retains the original audit context, prompt and supported verdict. Regression checks exact support metadata contains only the returned quote, while other example lines are rejection-only.
- Near-boundary test starts with an untrimmed context just below24000, then proves added support metadata triggers rejection-only trimming while preserving the question's concrete premises and cited general rule for inference.
- `CARGO_HOME="$PWD/.cache/cargo" cargo test --locked --offline`:158unit+6stdio passed;37+10 ignored.
- `cargo fmt --check` and `CARGO_HOME="$PWD/.cache/cargo" cargo clippy --locked --offline --all-targets -- -D warnings`: passed.

These checks prove context construction and guard preservation, not the new model's judgment. G07 generalization from an example and valid H02 question-premise inference require root's paired model review. No model or network calls were run by this agent.

## Explicit paired component

`quote_bound_live_saved_audit` makes exactly one audit model call per invocation. `RESEARCH_QUOTE_AUDIT_ARM=baseline` sends the exact saved full G09 context and original prompt; candidate uses production audit_context/AUDIT_PROMPT. Both use the existing model transport/schema. A baseline rerun may reject the old bad claim due to model stochasticity; retain that result alongside historical support, without relabeling it as the original result. No retries. Raw inputs and audit response are saved before parse/application; refusal leaves raw response and test error.

```sh
RESEARCH_QUOTE_AUDIT_ARM=candidate RESEARCH_GATE_REPLAY_DIR="$PWD/outputs/quote-audit-candidate" CARGO_HOME="$PWD/.cache/cargo" cargo test --locked --offline quote_bound_live_saved_audit -- --ignored --exact research::tests::quote_bound_live_saved_audit
```

Use a fresh output directory and serialize calls with root's other probes. An exit0 means processing completed, not semantic acceptance. Root must compare A1/A2 verdicts, returned witnesses and missing/status; preserve prior failed cohorts separately.
