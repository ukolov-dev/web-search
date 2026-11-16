# Missing followup: эксперимент3

Прототип исправляет узкую ветвь: draft/audit предлагает повтор исходной формы, уже использованной в первом раунде. Раньше search dedup завершал запрос как no_new_evidence при пустой очереди, несмотря на missing facet и оставшийся бюджет. Теперь эта форма дополняется текстом конкретного missing условия. При закрытом facet выбирается первый missing. Новый валидный next остаётся неизменным; пустой next сохраняет режим чтения очереди. Это не доказательство общего adaptive retrieval и не гарантия, что novel next релевантен missing.

Границы: сам прежний query validator проверяет site/path и длину, не обрезая текст. Слишком длинная combined форма и изменённый site вызывают ошибку, не ослабление guard. Строка сохраняет initial API/version и исходный site. Missing indices валидированы предыдущим этапом. Один дополнительный helper, no new model calls, прежние2forms/4reads/6generations/time/reserve/refusal gates.

Baseline ebdcc83. Детерминированный controller replay `repeated_followup_query_targets_missing_facet`: RED no_new_evidence, GREEN evidence_sufficient, ровно2forms/5generations. Fixture моделирует повтор общей темы, не выдаётся за реальный historical G07. G07/G08/F12 представлены отдельными строковыми API/RFC controls; site+version control проверяет сохранение Rust1.90 и doc.rust-lang.org/1.90.0.

Проверки с `CARGO_HOME="$PWD/.cache/cargo" CARGO_TARGET_DIR=/path/to/web-search/target`:

- `cargo test --locked --offline followup`:3passed.
- `cargo test --locked --offline`:172unit+6stdio passed до последнего эквивалентного refactor match→if; root повторяет final full suite.
- `cargo clippy --locked --offline --all-targets -- -D warnings`:passed.
- `cargo fmt --check`:passed.

Исторические ошибки разработки: anyhow macro namespace исправлен; clippy collapsible-match исправлен без clone после compile E0507. Они не считались behavioral failures.

Живые вызовы, сравнение с текущим planner на новом наборе и независимая приёмка не выполнялись: root замораживает общий candidate. Не увеличены бюджеты, не очищен refusal ledger. Окончательная полезность остаётся гипотезой до paired evaluation.
