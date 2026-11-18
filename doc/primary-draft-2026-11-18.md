# Новая генерация по первичному источнику, 18 ноября 2026

Проверяем другой этап: новый draft по одному сохранённому полному источнику → прежний audit, вместо переоценки прежних claims. Все четыре пробы завершены: 8 генераций, 4 local Qwen, ошибок/429/retry нет. Семантическая разметка сохранена отдельно.

Исходные вопросы T01 (itertools.groupby) и N04 (CPython 3.13 free-threaded), язык и actual model facets сохранены. Независимая gold-разметка содержит исходные пять условий; она не передаётся модели. Одна пара использует прежний Pynative/RealPython, другая — docs.python.org. Изменяются только источник и заново сгенерированные утверждения. Тот же DeepSeek и local Qwen, DRAFT_PROMPT/AUDIT_PROMPT/schema.

Это oracle-plan/source component: planner и SERP исключены, документ передан заранее. Один saved read и native Qwen на пробу; публичных source HTTP нет. groupby primary содержит 39110 символов, baseline71737. CPython оба источника ограничены100000 символами и truncated; нужные условия первичного источника присутствуют. Проверка не доказывает способность найти источник и не удовлетворяет требованию двух документов.

До запуска зафиксированы 42 файла: source/artifact hashes, original facets, gold, fixtures, Rust-код, driver и отдельная копия test binary. Snapshot7199d11, freeze SHA25649c4419725d90c15849b632f997a78331e96d768a306b54609e76cadfab2fbd3. Порядок D01primary,D02primary,D01baseline,D02baseline; один повтор, порядок не сбалансирован. Максимум4пробы/24генерации; до6генераций/180с на пробу,3072tokens на вызов, pacing10с внутри бюджета и10с между пробами. Первая ошибка/отказ останавливает серию; retry/resume/смены ключа или провайдера нет. Нормальный JSON/schema repair после успешного stop-ответа может использовать прежний общий бюджет; это не retry транспортной ошибки.

## Результат

Независимая [разметка](../feasibility/primary-draft-quality-2026-11-18.json): узкий gate PASS.

| Метрика | Прежний источник | Официальная документация |
| --- | --- | --- |
| Условия исходных вопросов | 2/5 | 5/5 |
| Полностью раскрытые вопросы | 1/2 | 2/2 |
| Ошибки / unknown в итоговых claims | 0 / 0 | 0 / 0 |
| Ошибочные составные claims до audit | 1 | 0 |

Прибавились три условия CPython: experimental, not-default, separate-executable. Потерь и новых false/unknown fingerprints нет. Проверены10из10собственных цитат по исходным Unicode-срезам и все4sourceSHA. Groupby baseline streaming/no-lookahead принят в заранее зафиксированном смысле отсутствия глобального просмотра; буквальное отсутствие внутреннего чтения следующего элемента не доказано.

Это два уже известных development-вопроса, один повтор с несбалансированным порядком. Времена вызовов и остатки живого Session-бюджета естественно различаются; одинаковы верхние лимиты и алгоритм, а не сериализованные contexts целиком. Такой результат позволяет продолжить проверку primary-first выбора, но не включать его как квалифицированный MCP-режим.

Во всех четырёх пробах выполнены draft и audit, по две генерации. MCP-подобный result имеет partial: каждый ответ использует один документ, а штатная достаточность требует два. Это не транспортная ошибка. Известная стоимость8вызовов — USD0,001327888; неизвестного usage нет.

В baseline CPython draft повторил устаревшее утверждение источника об обязательной собственной сборке. Audit отклонил составной claim из-за отсутствующей опоры для default-фасета; итоговый ответ пуст. Это не доказательство, что audit распознал именно фактическую ошибку custombuild.

## Диагностика 429

Однократный read-only GET `/api/v1/key` через прежний proxy вернул HTTP200: key/effective budget не исчерпаны. Ключ, label, точные суммы, идентификаторы и raw response не сохранялись. Это не восстанавливает причину исторических429 и не доказывает наличие текущей request quota. Поле rate_limit этого endpoint устарело. Основание: [официальная документация](https://openrouter.ai/docs/api/api-reference/api-keys/get-current-api-key).

Production `src/model.rs` теперь сохраняет HTTP prefix, только allowlisted limit_source либоunspecified и числовые/date Retry-After/Reset. Для numeric Reset единицы не предполагаются. Не выводятся rawbody/header/message/providername. Накопление≤16КиБ, ожидание≤min(1с,remaining/2); отдельный транспортный chunk может превышать лимит накопления. Диагностика не добавляет retry.

Тестовый Savedbackend дополнительно останавливается при любом transporterror, refusal/toolcall/non-stopfinish или cancellation, включая swallowed format-repair failure. Pendinghalt ставится до pacing/await. Это guard эксперимента: generic production repair_draft_text всё ещё может поглотить ошибку formatrepair; его поведение этим изменением не исправлялось.

## Проверки и границы

Rust snapshot7199d11:193unit+6stdio passed,46unit/10stdio ignored; fmt/clippy/release иdriver self-check прошли. Дополнительно6stdio прошли со свежимreleasebinary SHA2564212552249ee763819007e250a7c1a6c5158045f63c916024823aad8b09867b4. Это сборка и локальная проверка, не выпуск. Freeze42файла повторно совпал после серии; Rust/driver не менялись. Router smoke также прошёл.

Pre-live независимый code review сверил42/42SHA и все4fixtures. Проверяющий также реализовал небольшой cancellation guard, поэтому финальный общий review выполняется другим агентом. Первая версия cancellationunit не прошла из-за отсутствующего fixturekey; исправленный тест прошёл. HTTP429loopback проверка подтверждает ровно один POST и блокировку следующего; envelope/cancellation/pacing/diagnostic tests не обращаются к внешним сервисам.

Отдельно: изменение диагностики является production-кодом, primary-first generation остаётся test-only и не подключена к MCP. Новых зависимостей, поисковых API, обхода блокировок нет. Все прежние source refusals сохранены. Отсутствие429в этих8вызовах не доказывает, что pacing устранил историческую причину: отдельного A/B по лимиту не было. Полный поиск, исходные20/day2 и общая qualification не проверены; выпуска, push/merge/deploy нет.

[Cases](../feasibility/primary-draft-cases-2026-11-18.json), [results и latest source ledger](../feasibility/primary-draft-results-2026-11-18.json), [driver](../scripts/primary-draft-2026-11-18.py). Сырые source bodies, fixtures, model captures, Qwen traces, freeze и pre-live reviews сохранены в `feasibility/primary-draft-corpus-2026-11-18.tar.gz`; [размер/SHA](../feasibility/artifact-manifest.json). Бинарники и credentials в архив не входят.
