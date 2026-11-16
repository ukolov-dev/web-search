# Cross-section extraction experiment

Two original captured HTML documents were frozen before candidate coding: RFC9111 response no-store5.2.2.5→must-understand5.2.2.3 and real RFC9112 Content-Length6.2→Transfer-Encoding6.1. Source hashes, paired text hashes/counts and checks are in the companion JSON. No synthetic transfer is counted as real evidence.

The original heading-only prototype cannot uniquely select no-store because request/response headings tie. Therefore this experiment isolates a different explicit selection trigger: the question specifies section5.2.2.5 (or6.2). Both baseline and candidate receive the same HTML, URL and target section. Baseline contains the requested section only. Candidate additionally includes one hop of same-document referenced sections. This is not a claim that generic heading search improved.

Captured results: cache894→1404chars, sections5.2.2.5+5.2.2.3. Single-section output has only a link stating an exception exists; candidate includes the actual SHOULD-ignore condition. Framing663→8203chars, sections6.2+6.1 and extra bibliography13.1. The added6.1 includes sender/protocol and combined-header handling requirements absent from baseline. Extra bibliography is honest overfetch, not an extra recovered required fact. External RFC9110 references remain unresolved and are never fetched; one-hop closure does not establish global completeness.

## Prototype boundaries

Only test modules in src/main.rs change; production and public contract remain untouched. Same existing declared-RFC URL/meta/header guard and sanitizer. Local fragment IDs are matched exactly with uniqueness checks, resolved through an actual section or single-section wrapper/nearest containing section. Only leaf sections with a single heading are accepted; duplicates are withheld. No caller-controlled CSS selector or external requests. Already selected section IDs are deduplicated. Missing/ambiguous targets and over100000chars fall back to unchanged full-prefix extraction. TOC/sidebar targets removed before selection cannot rescue a candidate.

Selected output has its own text_hash and selected_sections; offset_scope explicitly means Unicode in selected extracted text, not full-body positions. Original clauses are copied from DOM, not rewritten. Existing DOM/text limits remain. This test-only implementation still calls baseline extraction for identity validation; that cost and lack of production wiring are explicit.

## Frozen questions and evidence

The initial source/section manifest and paired outputs remain unchanged in child outputs/section-exceptions/frozen.json and pairs/. Root subsequently required questions to explicitly ask about the linked caveat. A V2 question freeze was recorded **before any model calls**, preserving HTML/selected text and using identical question hashes in both arms. V2 paired JSON includes URL,title,question,HTML hash and extracted output. Copies are in root runs/product-options-followup-2026-11-16/fixtures/section-exceptions-{cache,framing}-{baseline,candidate}.json. This agent made no model calls.

The first full-HTML wrapper-resolution candidate missed must-understand because an outer ancestor section shadowed a wrapper's single child section. Its outputs are retained separately in pairs-wrapper-resolution-failed. The corrected resolver prioritizes the wrapper's unique descendant section, then containing section; unchanged full-HTML cases pass. Compilation mistakes during test construction are not red product evidence.

Tests cover bounded structurally faithful sections from both actual HTML documents; no-match, wrong domain, duplicate target IDs, TOC duplicate decoy and oversized linked section. Baseline/candidate assertions prove the single-section evidence loss; original fullbody is the source oracle, not successful live retrieval. Full offline suite, fmt/clippy and explicit captured pair replay passed. This does not establish model answer sufficiency or independent product acceptance. Root owns same-source Qwen/draft/audit and further review.
