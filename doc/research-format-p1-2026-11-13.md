# Format G07: bounded application repair

Реализован один format repair после application-level отклонения текста, без ослабления safe_claim_text/quote/RFC gates. Прежний schema repair этот путь не охватывал. DRAFT_PROMPT явно требует plain words для технических типов.

Контроллер сначала проверяет, что у format-rejected claim проходят остальные gates с безопасным текстом-пробой; исходная цитата не заменяется. После обычного pruning repair разрешён только при свободном общем self.repaired, generations<5 и remaining>2×FINAL_RESERVE. Он использует тот же immutable source context, не добавляет HTTP/Qwen calls или лимиты. После repair хорошие первоначальные claims остаются, новые проходят прежнюю валидацию; отдельный semantic audit по-прежнему обязателен. Ошибка/непригодный repair не стирает исходные хорошие claims. RFC literal rejection не обходится.

На baseline35765fe targeted controller regression воспроизвёл no_new_evidence вместо evidence_sufficient; тот же тест после causal fix проходит. Fixture `tests/fixtures/research/g07-format.json` содержит исходный candidate G07 model-03 draft/context из quote-binding-confirm corpus; оставлены только исходные S4 chunks/source, без подмены witness. Component replay принимает supplied plain-text None claim с теми же точными quotes. Это не новый живой прогон и не доказательство способности модели самостоятельно исправить ответ.

Проверки:

- `CARGO_HOME="$PWD/.cache/cargo" cargo test --locked --offline format_repair`:3 passed; guards общего allowance, 5-generation boundary, final time reserve, HTML negative и сохранение хорошего claim.
- `CARGO_HOME="$PWD/.cache/cargo" cargo test --locked --offline`:158unit+6stdio passed;35unit/10stdio ignored не запускались.
- `cargo fmt --check`:passed.
- `CARGO_HOME="$PWD/.cache/cargo" cargo clippy --locked --offline --all-targets -- -D warnings`:passed.

[Снимок и evidence](../feasibility/research-format-p1-2026-11-13.json). Public schema/source-state/selector unchanged. Реальные model/Qwen paired comparisons, independent reviewer и release checks выполняет root после интеграции; приёмка и выпуск здесь не заявляются.
