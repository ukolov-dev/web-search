# Documentation queue P1: offline исправление

G07 воспроизведён как ошибка порядка очереди: уже найденный Rust reference читался после GitHub releases и TensorFlow API. Минимальный reduced replay с сохранённым порядком трёх реальных URL проходит red→green. Полный factual G07 ещё требует format исправления и отдельного model сравнения; рост end-to-end recall не заявляется.

Добавлен один stable sort key после прежних RFC ключей. Только при явном RU/EN documentation intent и qualified API symbol предпочтение получают реальные candidates с точным container token в URL path и reference-looking path/host. Для Rust дополнительно требуется Rust token в host/path, чужой framework не получает это предпочтение. Explicit method fragment другого метода не предпочитается. Полные страницы container без method fragment допустимы для последующего чтения. Никакие URL не генерируются, доменный whitelist и новые calls не добавлены. Это эвристика retrieval, не доказательство официальности/поддержки claim.

Негативные контроли: container только в hostname/query; VecDeque вместо Vec; foreign framework; insert_mut вместо insert в fragment; ordinary/news; explicit site. Полный qualified std::vec::Vec::insert разбирается до Vec. Прежние query site/path tests проходят. Новая логика не меняет query rewrites, URL/DNS/IP/redirect/source-state/cache фильтры, бюджеты, extraction/Qwen/news selection, Unicode/witness/audit.

Проверки:

- `CARGO_HOME="$PWD/.cache/cargo" cargo test --locked --offline documentation_queue_preserves_api_container_before_unrelated_results`: RED GitHub != Rust reference, затем GREEN1.
- `CARGO_HOME="$PWD/.cache/cargo" cargo test --locked --offline`:157unit+6stdio passed,35+10ignored.
- `CARGO_HOME="$PWD/.cache/cargo" cargo clippy --locked --offline --all-targets -- -D warnings`: passed.
- `cargo fmt --check`: passed.

Снимки файлов и исторические compile/mock ошибки сохранены в [evidence](../feasibility/research-docs-p1-2026-11-13.json); не скрыты как успешные product проверки. Корпус: quote-binding-confirm-corpus-2026-11-13.tar.gz, candidate/new/G07 model-01-plan, serp-01 и result. CLI scoped pack/reuse выполнены в этом worktree; Router lifecycle ведёт root.

Не запускались public/model/Qwen/Harness пробы и release build; root проводит отдельное frozen paired сравнение и независимую приёмку после интеграции. Малый offline replay не закрывает общий acceptance gate.

## Review follow-up: punctuation

Отдельный reviewer обнаружил, что `Rust:`/`Rust,` не включали namespace hint и чужой framework получал preference. Focused `documentation_language_hint_survives_punctuation` воспроизвёл RED. Исправление переиспользует существующий tokenizer вместо whitespace equality. `cargo test --locked --offline documentation_` (с прежним CARGO_HOME) проходит4tests; fmt/clippy прошли. Полную suite заново выполняет root после интеграции.
