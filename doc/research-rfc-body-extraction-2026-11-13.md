# RFC body extraction: F03 causal replay

The captured F03 raw RFC9111 HTML contains the no-store response clause (`immediate request`, private/shared applicability). Existing extraction selects Readability because modern xml2rfc markup has neither main nor article. Offline extract-file produces exactly the historical54294-character text, truncated=false, omitting the normative clause. This is an extraction miss before Qwen or drafting, not a coverage judge error.

A narrow declared-body path now requires a canonical RFC URL through the existing RFC URL parser, unique matching rfc.number, xml2rfc generator, matching h1#rfcnum, nonempty h1#title and matching document title. Only then is the original body selected before Readability. Generic pages, wrong domain/number/title/header, missing declaration or other generators do not gain this path. DOM50000 and extracted text100000 caps, sanitization, URL/network guards and public APIs remain unchanged. Existing rfc_url is exposed only to the parent module for reuse.

Original captured HTML replay now yields declared_rfc_body73476chars, truncated=false, with immediate-request clause and private/shared wording retained. Own RFC9111 heading is preserved within first2048chars, restoring existing primary_rfc identification without relaxing that guard. The tracked5.8KB fixture is a structurally faithful subset containing the original modern RFC declarations/header and no-cache/no-store/no-transform sections. Full captured HTML and full replay outputs remain ignored.

Extraction cache namespace/key changes extract-v1→extract-v2 to prevent reuse of old lossy text. This causes cold extraction for previously cached pages; HTTP cache policies/validators/refusal guards are unchanged.

Checks: extraction regression red against disabled new branch, green with branch; wrong identity/domain/meta/generator negatives, explicit100000 truncation control and modern-body primary RFC identity regression. Full offline suite and clippy pass; counts in linked [JSON](../feasibility/research-rfc-body-extraction-2026-11-13.json).

No HTTP/model/Qwen calls. F02 RFC9110 remains subject to the100000-character boundary; this change does not recover clauses beyond that cap or prove full answer sufficiency. Previously failed comparisons remain historical failures. Root owns fresh model/runtime acceptance.
