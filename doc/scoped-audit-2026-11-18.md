# Узкий audit: объект/версия, время и условия

**Кандидат не прошёл критерий улучшения:** пропустил ложное H01, которое baseline отклонил, и потерял правильное H02. Из32 запланированных модельных вызовов выполнено15; на S04 candidate существующий guard отверг невалидный конфликт. Остальные17 не запускались, повторов не было. Эксперимент частичный, продукт не принят.

## Что проверяли

После неудачного witness A/B проверен отдельный вариант **существующего** audit, без переписывания claim или замены доказательства. В [test-only модуле](../src/research_scope_audit_experiment.rs) baseline использует текущие RULES/AUDIT_PROMPT и Audit schema. Candidate получает те же инструкции плюс явные проверки `object_version`, `time`, `conditions`, `own_quote_support` и ScopeAudit schema.

Для сохранения claim обязательна own_quote_support=supported; остальные применимые оси должны быть supported. `unknown`/`unsupported` запрещают сохранение. `not_applicable` допускается только для неприменимой оси; отсутствующая опора не равна N/A. Adapter проверяет exact IDs и переводит результат в обычный Audit; существующий apply_audit и его guards сохранены.

Это эксперимент с модельным решением, не детерминированный semantic verifier. Три заполненных поля не доказывают истинность. Исходная цитата сохраняется в обеих arms: расширение контекста или extractive replacement не применялись, чтобы изолировать audit. Хороший факт может потеряться из-за нового модельного решения — именно это измеряется.

Новый код подключён **только в cfg(test)** через research_tests.rs. Production research.rs/model.rs, CLI/MCP контракты и runtime default не менялись. Источник реализации `29be5be`, база `2b2310b`; snapshot файлов/компилированного test binary зафиксирован в freeze. В executable продукта кандидат не включён.

## Материалы, границы и вызовы

[Cases/gold](../feasibility/scoped-audit-cases-2026-11-18.json), [результаты и полный список not_run](../feasibility/scoped-audit-results-2026-11-18.json), [независимые semantic/code reviews](../feasibility/scoped-audit-quality-2026-11-18.json), [driver](../scripts/scoped-audit-2026-11-18.py).

- Historical development8: прежние записанные question/claim/witness, без изменения формулировок.
- Новый synthetic8: отдельные fictional API-пары по объекту, версии, датированному/текущему факту, условию и модальности. Это не web holdout и не измерение качества поиска.
- Gold, входы, код и фактические contexts заморожены до модельных вызовов. Авторы модуля новый challenge не читали по их заявлению; это процедурная независимость. Gold в model inputs не передавался.
- Title/publication/document_version исторических источников восстановлены по URL+full source hash из прежних records. Synthetic с null URL получили явно условные example.org адреса без сетевого обращения. Original offsets и тексты сверены отдельно.
- На case одна frozen claim/own quote: собственная цитата занимает отдельный исходный chunk; соседние source chunks используются только как rejection context. До3chunks, каждый≤1200chars, весь context≤24000chars. Текст исходный, не синтезированный из gold. Оба arms получают **идентичные serialized context bytes**, включая frozen clocks/budget fields.
- Это новая явно разрешённая серия, а не продолжение остановленной429-кампании. Модель прежняя `deepseek/deepseek-v4.1-flash-1017`, proxy/key configuration прежние. Источники не перечитывались, sourceHTTP0; отказные host states не сбрасывались и не менялись.
- По одному model call на arm/case, максимум32;90с/3072outputtokens на вызов, pacing3с. Порядок H/S чередуется, порядок baseline/candidate тоже меняется по case. Ограничение серии600с реализовано admission reserve108с перед следующим вызовом, не внешним hardwall всего процесса.
- Первая ошибка/невалидный результат останавливает серию; никаких retry, автоматического repair или изменения prompt после просмотра ответов. Existing production shared repair/controller здесь не выполняется.

Перед парсингом проверяются finish_reason=stop, отсутствие refusal/tool_calls; одинаково для обеих arms. Raw ответ сохраняется до проверки. Это дополнение выявлено независимым pre-live review и проверено отдельным unit.

## Фактический исход

Получено **15 ответов провайдера**, из них14 прошли применение audit; **7 завершённых пар**:4historical и3synthetic. Все15 ответов содержат usage; ответная модель совпадает с запрошенной. HTTP429 в новой серии не было.

| Завершённые пары | Показатель | Baseline | Candidate |
| --- | --- | --- | --- |
| Historical4 | Сохранённые корректные claims из1 испытанного положительного | 1/1 (H02) | 0/1 |
| Historical4 | Сохранённые false | 0 | 1 (H01) |
| Historical4 | Сохранённые unknown | 1 (H04) | 1 (H04) |
| Synthetic3 | Положительные сохранены | 2/2 (S01/S03) | 2/2 |
| Synthetic3 | Отрицательный S02 отклонён | да | да |

**Historical gate FAILED** одновременно по no-new-false и no-positive-loss. Непроверенные H05–H08 не засчитываются как потери. В frozen8 было5 хороших claims; проверен только один — H02, поэтому 1/1/0/1 выше не является общей оценкой retention на5контролях. H04 остаётся неопределённым из-за потери scope, а не подтверждённым фактом.

H01 содержит необоснованное требование предварительной сортировки groupby. Новый audit отмечает его supported по осям и сохраняет, тогда как baseline отклоняет. H02 корректно ограничивает сортировку задачей собрать все одинаковые ключи; candidate его отвергает. Наличие правильной текстовой формы ответа/JSON не предотвратило обе ошибки.

**S04 candidate:** модель вернула валидный JSON и raw checks=unsupported. Но вместе с ним объявила конфликт между S1P1/S1P2 — двумя фрагментами **одного** документа, содержащими разные version scopes. Existing guard `conflict requires two distinct document bodies` остановил apply_audit. Result отсутствует, accepted keep=null: raw unsupported не считается успешно применённым отказом или исправлением.

После этого не выполнены baseline S04, обе arms H05–H08 и S05–S08: **17 not_run**. Новые synthetic проверки времени и условной модальности поэтому не завершены. Source/refusal/ошибки генерации не подменяются метрикой правильного смыслового отказа.

Для сохранности полностью frozen denominators в quality также записаны observed conditions H2/15→1/15 и S2/10→2/10. Эти числа включают непроверенную часть наборов; не являются полным recall или сравнением поиска. Когорты не объединяются.

Известная суммарная стоимость всех15 вызовов, включая невалидный S04: **USD0,001348336**, неизвестных usage0. Median arm call elapsed baseline1538мс, candidate1950мс; pacing не включён, один запуск не является latency benchmark или причинным доказательством влияния schema.

## Вывод и ограничения

Разделение audit на оси в этом варианте **не решило semantic проблему** и не сохраняет правильные ответы. Источник может сам содержать неверное обобщение; точное совпадение claim с ним и заполненная форма не устанавливают мировую истинность. При этом одиночные результаты не доказывают общую причинность каждого ошибочного решения. Candidate prompt включает прежний audit плюс scoped override, так что отдельно изолирован эффект всего варианта, а не каждой оси.

Следующий шаг нельзя объявлять доказанным улучшением. Сначала нужно разбирать расхождение claim, условий задачи и авторитетной исходной опоры; в этом эксперименте новые источники/контрдоказательства не искались. Положительные S01/S03 остаются только компонентными контролями, не обоснованием выпуска.

Сохранены первоначальные tooling errors до network: crate::save требовал serde_json::Value; null synthetic URL вызвал old_session panic. Исправления выполнены до freeze и model calls, не скрыты повторами провайдера. Финальный prepare16 завершён без ключа/Model.

Важный предел helper: old_session использует placeholder generation/model-call counters0 и пустой source.text_hash. Фактические15 вызовов установлены runner ledger и raw audit.json, а исходные hash/version/witness — проверены через frozen inputs и original captures. Placeholder fields не доказывают production budget wiring, cache hit или live source read. В обеих arms одинаковый material, но это component replay, не end-to-end MCP.

Обязательные offline Rust/driver/Router проверки и независимая оценка записаны отдельно в verification evidence. Реализация test-only есть; приёмка улучшения не пройдена, эксперимент частичный. Нет push, merge, deploy, нового API/модели или изменения общих бюджетов. Исходные20/day2, полная квалификация и устойчивость к повторным запускам не проверялись.

Raw inputs, prepare/calls/errors/contexts/freeze/reviews сохранены локально в `feasibility/scoped-audit-corpus-2026-11-18.tar.gz`, размер/SHA в [манифесте](../feasibility/artifact-manifest.json); бинарник и credentials в архив/Git не включены. Latest cumulative source refusal ledger остаётся `feasibility/search-repair-results-2026-11-18.json:source_state_after`.
