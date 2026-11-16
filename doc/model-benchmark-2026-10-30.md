# DeepSeek Flash и локальная MiMo: парный benchmark — 30.10.2026

**DeepSeek увеличил recall, но одновременно увеличил число ложных direct. Цель «поднять recall без новых ложных direct» простой заменой модели не достигнута.**

На основном полностью размеченном SERP-срезе recall после общих guards вырос с **52,8% до 69,4%**: 25 TP вместо 19, но **10 FP вместо 6**. На отдельном replay чтения восьми вопросов recall **45% → 80%**, FP **2 → 4**. По всей ветке DeepSeek OpenRouter сообщил **$0,172478** за 73 генерации. Обе модели дали 46/46 валидных исходных классификаций; у DeepSeek один ответ body-этапа отклонён контрактом.

Для задач, где важнее полнота, DeepSeek — перспективный основной кандидат. Для строгого требования минимальных ложных direct результаты требуют дополнительной проверки семантического покрытия. В этом benchmark guards и prompts не менялись. Это сравнение с прежней авторской разметкой, а не независимая квалификация качества.

Машинные [результаты](../feasibility/model-benchmark-results-2026-10-30.json) включают все вопросы, confusion matrices, ошибки, latency, токены, стоимость, интервалы и FP/FN-примеры. [Corpus](../feasibility/model-benchmark-corpus-2026-10-30.json) содержит зафиксированные данные и прежние суждения.

## Основной полностью размеченный SERP-срез: 18 вопросов / 318 карточек

| Этап | Модель | TP | FP | FN | Precision direct | Recall direct |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Ответ модели | MiMo 9B | 26 | 24 | 10 | 52.0% | 72.2% |
| Ответ модели | DeepSeek Flash | 29 | 19 | 7 | 60.4% | 80.6% |
| После guards | MiMo 9B | 19 | 6 | 17 | 76.0% | 52.8% |
| После guards | DeepSeek Flash | 25 | 10 | 11 | 71.4% | 69.4% |

## Чтение сохранённых страниц: Recall8 / 142 карточки

| Этап | Модель | TP | FP | FN | Precision direct | Recall direct |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| SERP + guards | MiMo 9B | 8 | 2 | 12 | 80.0% | 40.0% |
| SERP + guards | DeepSeek Flash | 10 | 2 | 10 | 83.3% | 50.0% |
| После body replay | MiMo 9B | 9 | 2 | 11 | 81.8% | 45.0% |
| После body replay | DeepSeek Flash | 16 | 4 | 4 | 80.0% | 80.0% |

## Original20: только 200 ранее размеченных top-10 карточек

| Модель после guards | TP | FP | FN | Precision на размеченном подмножестве | Recall на размеченном подмножестве | Direct вне разметки |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| MiMo 9B | 28 | 9 | 57 | 75.7% | 32.9% | 18 |
| DeepSeek Flash | 62 | 22 | 23 | 73.8% | 72.9% | 52 |

## Неопределённость разницы DeepSeek − MiMo

| Срез | Разница recall | 95% bootstrap-интервал разницы |
| --- | ---: | --- |
| SERP18 + guards | +16.7 п.п. | -6.8 … +37.2 п.п. |
| Recall8 + body | +35.0 п.п. | +12.5 … +75.0 п.п. |
| Original20, judged subset | +40.0 п.п. | +23.4 … +59.2 п.п. |

## Доступность и стоимость

| Модель | Валидных SERP вопросов | Модельных попыток | Body-ошибок | Медиана SERP-вопроса | P95 SERP-вопроса | Перезагрузка вне таймера | Reported cost |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| MiMo 9B | 46/46 | 74 | 0 | 26.68 с | 55.98 с | 170.8 с | API-биллинг отсутствует; hardware не измерялся |
| DeepSeek Flash | 46/46 | 73 | 1 | 17.10 с | 34.53 с | 0.0 с | $0.172478 |


## Что означают результаты

**До guards DeepSeek лучше на обеих метриках SERP18:** precision 60,4% против 52%, recall 80,6% против 72,2%. После одинаковых формальных проверок локальная модель получает более высокую precision (76% против 71,4%), но теряет больше recall. Guards убрали 18 из 24 локальных FP и 7 TP; у DeepSeek — 9 из 19 FP и 4 TP. Подлинность цитаты и наличие API/date токенов не доказывают семантическую полноту карточки.

В основном SERP-срезе все 10 DeepSeek FP и 6 MiMo FP дошли до top-10 — это не только внутренние метки пула. При этом известных пригодных источников в top-10 у DeepSeek 31 против 28, в top-3 — 21/54 позиции против 17/54. Это эффект ранжирования фиксированного пула, не увеличение охвата интернета.

На Recall8 DeepSeek повысил восемь дополнительных карточек: **6 TP и 2 FP**. MiMo повысила одну карточку, она TP. Все 20 известных пригодных страниц уже присутствовали в top-10 у обеих моделей; после body replay изменились метки и порядок, не полнота этого top-10. Пригодные позиции top-3: 15/24 против 13/24.

Два новых body-FP DeepSeek — `recall8-f02/C010` ([CTE в PostgreSQL](https://hakibenita.com/be-careful-with-cte-in-postgre-sql)) и `C014` ([изменения PostgreSQL 17](https://www.pgedge.com/blog/postgresql-17-a-major-step-forward-in-performance-logical-replication-and-more)). В фиксированном эталоне они background: доступное свидетельство не раскрывает всё сравнение MATERIALIZED / NOT MATERIALIZED и optimization fence. Цитаты существуют, однако второй модельный аудит их принял. Это показывает ограничение текущего relevance audit; factual truth здесь отдельно не проверялась.

**Другие характерные расхождения с gold:**

- `development12-d07/C001`: сравнение CSV/NDJSON в общем контексте хранения принято DeepSeek, хотя в имеющемся фрагменте не раскрыта запрошенная потоковая обработка.
- `development12-d09/C015`: совет о modified/untracked worktree принят для вопроса о locked/force; требуемое различие locked-состояния не подтверждено.
- `development12-d10/C013`: материал о RC1 и запланированной дате принят вместо состоявшегося официального анонса PostgreSQL 17.
- `followup6-h01/C019`: заголовок и обещание руководства по 3xx приняты без явного сравнения сохранения метода для 307/308.

Названия FP/FN означают расхождение с фиксированной старой шкалой. Есть и спорные суждения: например, общий обоим моделям `development12-d03/C001` действительно содержит противопоставление viewport/container, но прежний gold равен 1. Такие записи не исправлялись после просмотра предсказаний. Для сильного вывода о реальном качестве нужна независимая повторная разметка.

### Sensitivity и статистические ограничения

Если понизить пять заранее отмеченных пограничных SERP-positive в Recall8, после body replay у MiMo TP5/FP6/FN10, precision45,5%, recall33,3%; у DeepSeek TP12/FP8/FN3, precision60%, recall80%. Вывод о приросте полноты сохраняется; абсолютные precision и FP чувствительны к эталону.

Основной SERP18 прирост recall +16,7 п.п., но 95% query-bootstrap интервал **−6,8…+37,2 п.п.** пересекает ноль. Разница precision −4,6 п.п. имеет интервал −20,9…+6,8 п.п. Для Recall8 интервал прироста recall +12,5…+75 п.п. положителен, но это восемь уже использованных development-вопросов со смешанным pooling. Интервалы описывают этот corpus; они не превращают прежнюю авторскую разметку в независимую проверку.

### Практическая рекомендация

Использовать DeepSeek для большей полноты можно, сохранив текущие guards и явное различие background/direct. Считать проблему ложных direct решённой нельзя. Следующий отдельный эксперимент — проверка составных требований и отрицательных различий: пары SQL-директив, locked против dirty worktree, planned против released, общая тема против конкретного режима обработки. Менять эти правила по текущим ответам внутри этого benchmark не стали.

## Время, токены и доступность

DeepSeek: 261302 input и 143526 completion tokens; из последних OpenRouter отметил 134995 как reasoning. Все 73 вызова обслужил OpenInference с ID `deepseek/deepseek-v4.1-flash-1017`; отсутствующих cost нет. Локально: 175433 input, 43471 completion, reasoning0. Токенизаторы и режимы reasoning разные, поэтому токены не являются одинаковыми единицами вычислений.

Медиана одного начального batch: DeepSeek15,28с, MiMo24,77с; P95 19,34с / 33,33с. В таблице выше время вопроса учитывает несколько batch, но исключает retrieval и preflight; это model-only benchmark. Перезагрузка oMLX заняла ещё 170,8с суммарно и записана отдельно. Ленивая инициализация весов/префилл могут входить во время первого вызова; существующие SSD/prefix cache не очищались. Это операционное сравнение текущих endpoints, не изолированный тест архитектур или полностью тёплой сессии.

У обеих моделей нет HTTP/model отказов в этой серии. Единственный семантический отказ контракта — DeepSeek `recall8-f05`: указал passage для background. Parser отверг body-ответ, повышения не выполнялись, исходные SERP-метки сохранились. Повторной платной генерации не делалось. У MiMo нет body-ошибок; отсутствие memoryguard400 доказано только с контролируемой перезагрузкой перед каждым вопросом.

API-биллинг локального сервиса отсутствует, но электричество, ресурсы Mac и амортизация не оценивались. $0,172478 — сумма reported usage.cost этой remote-ветки, не стоимость других прежних экспериментов.

## Проверки воспроизводимости

- 46/46 вопросов и 59 начальных batch парно сопоставлены по фактическим messages, response_format, temperature и max_tokens — совпадение точное. Corpus, prompt и тестовый бинарник имеют одинаковые хеши в обеих ветках. Провайдерские параметры reasoning/transport различаются, как указано в протоколе.
- Все 660 прежних judgment IDs проверены на соответствие candidate IDs; grade/URL сохранены. Source export/RRF из старых source snapshots совпал с зафиксированным candidate pool; gold не попал в model input.
- Corpus SHA-256: `6481dc32068f8e41dfd5f68c886ffb799c61caea7179ef9fd888b4fb0d47101d`. Исходные 20 запросов не менялись; их SHA: `b67e3269830ae0e976aa9b4c5dc2f6b9c3e922ad1adda35738289d6b1b8325d6`.
- Пройдены 37 обычных тестов, Clippy all-targets -D warnings, арифметический self-check confusion matrices и структура Markdown-таблиц. Сырые calls, lifecycle, manifests, скрипты анализа и исполнявшиеся Rust-исходники сохранены в `runs/model-benchmark-2026-10-30/`.
- Runtime поиска/классификации в этом задании не менялся. Rust benchmark и helper перезагрузки компилируются только с cfg(test). Новых поисковых запросов и target reads — 0. Это не day2 общего поиска, не load test и не независимая приёмка полного пилота.

## Подробный протокол

## Вопрос и протокол

Сравниваются `deepseek/deepseek-v4.1-flash-1017` через OpenRouter и локальная `MiMo-V2.6-Distill-Qwen-9B-OptiQ-4bit` в oMLX. Задача — оценить recall метки direct, ложные direct до/после общих guards и эффект чтения сохранённых страниц. Это benchmark текущих конфигураций приложения: у MiMo thinking отключён, DeepSeek использует провайдерский reasoning default; общий max_tokens включает его внутренние reasoning-токены. Это не эксперимент с одинаковым внутренним режимом рассуждения.

Все 46 запросов, 1058 кандидатов, существующие суждения и источники зафиксированы до новых вызовов. Обе модели получают одинаковые query/title/snippet, строгую JSON Schema и тот же conservative prompt. Gold не передаётся модели. После исходных ответов применяется общий Rust all-ID/literal/verbatim quote validator. Промпты, наборы и суждения во время прогона не подстраиваются.

Новых обращений к поисковикам или сайтам нет: используются сохранённые SERP от 29.10.2026. Поэтому это ретроспективный model benchmark, а не новый live/day2 поиск. Original20 и frozen retrieval forms не изменяются.

## Разметка и границы метрик

| Группа | Запросы | Карточки | Размечено | Grade2 | Основание |
| --- | ---: | ---: | ---: | ---: | --- |
| Development12 | 12 | 213 | 213 | 28 | Собственная старая SERP-разметка |
| Followup6 | 6 | 105 | 105 | 8 | Собственная старая SERP-разметка |
| Recall8 | 8 | 142 | 142 | 20 | Смешанная SERP/прочитанный текст, пять пограничных SERP-positive |
| Original20 | 20 | 598 | 200 | 85 | Только прежний top-10, не весь пул |
| Итого | 46 | 1058 | 660 | 141 | 398 карточек не имеют gold |

Основной SERP-срез — 18 запросов с полностью размеченными 318 карточками. Отдельно показаны 8 вопросов со смешанными основаниями и original20 на размеченном подмножестве. Предсказанные direct на 398 непроверенных карточках не считаются FP. Для original20 это условная оценка на top-10, выбранном прежним ранжированием, со смещением отбора; объединять её с полными пулами в одну headline-precision нельзя.

Gold составлен автором реализации в предыдущих задачах, не независимым разметчиком. Большинство данных уже использовалось при разработке; это не новый holdout. Сомнительные positive проверяются отдельной sensitivity-оценкой, а не переписываются под новую модель.

## Дополнительный replay чтения

Только для Recall8 обе ветки используют один зафиксированный набор ранее прочитанных документов. Каждая модель может повысить свои background-карточки после того же выбора source passages и второго аудита. Кандидатное множество cached-страниц одинаково; конкретные inputs body-этапа могут различаться из-за разных initial labels. Это естественный эффект замены модели в pipeline.

Чтение новых сайтов не выполняется. Набор документов первоначально выбран локальной моделью при разработке, поэтому не является беспристрастной оценкой всего процесса поиска/чтения. Максимум три модельных вызова на случай и 180 секунд сохранены. Отказ body-этапа оставляет исходные SERP-метки, не создаёт новых direct.

## Доступность, задержка и стоимость

Перед локальным вопросом штатным API oMLX перезагружается только наша MiMo, строго при нуле active/waiting запросов, отсутствии загрузки и других загруженных моделей. Чужие запросы не прерываются. Глобальные настройки и memory guard tier не меняются. Время unload/load записывается отдельно и исключено из inference latency. Это контролируемое состояние; устойчивость непрерывной тёплой сессии им не доказана.

DeepSeek запускается через ранее настроенный локальный HTTPS proxy и закреплённый ID. Endpoint, ключ и маршрут не управляются input-инструментами MCP. Ключи и авторизационные настройки не помещаются в corpus или отчёт. Сохраняется фактический usage.cost; отсутствующая стоимость отмечается unknown. До 90 remote calls, остановка при reported cost $2, трёх unknown cost или 403/429. Автоматических повторов и fallback нет.

Основные quality-таблицы считаются на общих валидных случаях. Отдельная operational-оценка учитывает отсутствие успешной классификации как отсутствие direct, не как утверждение irrelevant. JSON/finish/coverage/model errors сохраняются. Для неопределённости используется парный bootstrap по вопросам; RU/EN-пары original20 кластеризуются вместе. Интервалы описывают этот корпус, а не независимую генеральную выборку.

## Воспроизводимость

Данные: [frozen corpus](../feasibility/model-benchmark-corpus-2026-10-30.json). Диагностика и сырые model calls находятся в `runs/model-benchmark-2026-10-30/`, отдельно deepseek/local. Manifest каждой ветки содержит хеш corpus, prompt и исполняемого тестового бинарника. Pipeline runtime не перенастраивается: новый Rust harness и local reload helper компилируются только с cfg(test).

В отчёте и машинной сводке сохранены общий valid coverage, TP/FP/FN/TN, precision/recall/FPR, raw → guarded → body, latency, usage/cost, bootstrap, случаи разногласий и ограничения. Независимая приёмка и общая квалификация пилота этим benchmark не закрываются.


## Все вопросы: после guards и доступного body replay

Ячейка модели: TP / FP / FN на имеющейся разметке; для Original20 это только старый top-10.

| ID | Вопрос | Gold2 | MiMo TP/FP/FN | DeepSeek TP/FP/FN |
| --- | --- | ---: | --- | --- |
| original20-ru01 | Rust владение borrowing документация | 6 | 3/2/3 | 3/2/3 |
| original20-ru02 | SQLite транзакции WAL документация | 4 | 1/0/3 | 2/0/2 |
| original20-ru03 | HTTP статус 304 Last-Modified ETag | 5 | 2/0/3 | 3/0/2 |
| original20-ru04 | Tokio асинхронный Rust руководство | 5 | 2/0/3 | 5/4/0 |
| original20-ru05 | экскаватор рабочий цикл перемещение грунта | 4 | 0/0/4 | 4/3/0 |
| original20-ru06 | site:doc.rust-lang.org владение ownership | 7 | 3/0/4 | 7/1/0 |
| original20-ru07 | Rust Python обработка HTML сравнение | 1 | 0/1/1 | 0/0/1 |
| original20-ru08 | последняя стабильная версия Rust октябрь 2026 | 0 | 0/0/0 | 0/0/0 |
| original20-ru09 | Unicode графема кодовая точка разница | 4 | 0/0/4 | 2/2/2 |
| original20-ru10 | DNS rebinding SSRF IPv6 защита | 5 | 0/0/5 | 4/1/1 |
| original20-en01 | Rust ownership borrowing documentation | 7 | 0/0/7 | 6/3/1 |
| original20-en02 | SQLite WAL transactions documentation | 3 | 0/0/3 | 1/0/2 |
| original20-en03 | HTTP 304 ETag Last-Modified conditional requests | 7 | 2/0/5 | 2/0/5 |
| original20-en04 | Tokio Rust asynchronous programming tutorial | 5 | 5/5/0 | 4/0/1 |
| original20-en05 | excavator working cycle digging loading travel | 4 | 0/0/4 | 4/2/0 |
| original20-en06 | site:doc.rust-lang.org ownership | 6 | 6/1/0 | 6/1/0 |
| original20-en07 | Rust Python HTML extraction comparison | 1 | 0/0/1 | 0/3/1 |
| original20-en08 | latest stable Rust release October 2026 | 0 | 0/0/0 | 0/0/0 |
| original20-en09 | Unicode grapheme cluster code point difference | 7 | 3/0/4 | 5/0/2 |
| original20-en10 | DNS rebinding SSRF IPv6 protection | 4 | 1/0/3 | 4/0/0 |
| development12_full_SERP-d01 | PostgreSQL SELECT FOR UPDATE SKIP LOCKED очередь задач документация | 1 | 0/0/1 | 0/0/1 |
| development12_full_SERP-d02 | HTTP 412 If-Match optimistic concurrency documentation | 3 | 2/0/1 | 2/0/1 |
| development12_full_SERP-d03 | CSS container queries media queries сравнение | 7 | 2/2/5 | 5/2/2 |
| development12_full_SERP-d04 | site:docs.python.org/3/library asyncio TaskGroup ExceptionGroup | 0 | 0/0/0 | 0/0/0 |
| development12_full_SERP-d05 | Python 3.13 free threading GIL release October 2024 | 1 | 0/0/1 | 0/0/1 |
| development12_full_SERP-d06 | robots.txt RFC 9309 crawl delay supported specification | 3 | 2/0/1 | 3/0/0 |
| development12_full_SERP-d07 | JSON Lines NDJSON CSV сравнение потоковая обработка | 1 | 1/0/0 | 1/1/0 |
| development12_full_SERP-d08 | Brotli gzip compression comparison static web assets | 6 | 5/2/1 | 5/2/1 |
| development12_full_SERP-d09 | git worktree remove locked force документация | 0 | 0/2/0 | 0/3/0 |
| development12_full_SERP-d10 | PostgreSQL 17 release September 2024 official announcement | 0 | 0/0/0 | 0/1/0 |
| development12_full_SERP-d11 | HTTP Retry-After 429 503 документация | 5 | 4/0/1 | 5/0/0 |
| development12_full_SERP-d12 | site:developer.mozilla.org structuredClone transferable ArrayBuffer | 1 | 0/0/1 | 0/0/1 |
| followup6_full_SERP-h01 | HTTP 307 308 redirect method preservation difference documentation | 3 | 0/0/3 | 2/1/1 |
| followup6_full_SERP-h02 | site:sqlite.org foreign_keys PRAGMA transaction no effect documentation | 0 | 0/0/0 | 0/0/0 |
| followup6_full_SERP-h03 | Python pathlib Path.resolve Path.absolute symlink difference | 0 | 0/0/0 | 0/0/0 |
| followup6_full_SERP-h04 | Redis 6.0 release April 2020 ACL official | 0 | 0/0/0 | 0/0/0 |
| followup6_full_SERP-h05 | PostgreSQL vacuum analyze autovacuum differences documentation | 1 | 0/0/1 | 1/0/0 |
| followup6_full_SERP-h06 | HTTP Content-Security-Policy Report-Only enforcement отличие документация | 4 | 3/0/1 | 1/0/3 |
| recall8-f01 | HTTP 301 302 307 308 redirects request method preservation difference | 3 | 0/0/3 | 3/0/0 |
| recall8-f02 | PostgreSQL CTE MATERIALIZED NOT MATERIALIZED optimization fence documentation | 1 | 1/0/0 | 1/2/0 |
| recall8-f03 | Python asyncio.TaskGroup ExceptionGroup cancellation documentation | 1 | 0/0/1 | 1/1/0 |
| recall8-f04 | CSS box-sizing border-box content-box padding border difference | 5 | 4/1/1 | 4/0/1 |
| recall8-f05 | site:docs.python.org tempfile NamedTemporaryFile delete_on_close Windows documentation | 1 | 0/0/1 | 0/0/1 |
| recall8-f06 | Rust std::sync::Mutex poison PoisonError recovery documentation | 3 | 2/0/1 | 3/0/0 |
| recall8-f07 | Go 1.23 release August 2024 range over functions iteration | 1 | 0/0/1 | 1/0/0 |
| recall8-f08 | git reset --soft --mixed --hard HEAD index working tree differences | 5 | 2/1/3 | 3/1/2 |


## Команды прогона и пересчёта

Выполненный harness: `benchmark::paired_model_benchmark` (именованный ignored-тест). Для повторной серии использовать новый родительский каталог и два несуществующих output-каталога; существующие результаты не удалять. Пример запуска, где `runs/model-benchmark-repeat` заранее создан:

```sh
WEB_SEARCH_BENCH_PROVIDER=openrouter \
WEB_SEARCH_BENCH_OUTPUT=runs/model-benchmark-repeat/deepseek \
WEB_SEARCH_OPENROUTER_KEY_FILE="$HOME/.config/web-search/openrouter.key" \
WEB_SEARCH_OPENROUTER_PROXY=http://127.0.0.1:10808 \
CARGO_HOME="$PWD/.cache/cargo" \
cargo test --locked --offline benchmark::paired_model_benchmark -- --ignored --exact --nocapture

WEB_SEARCH_BENCH_PROVIDER=omlx \
WEB_SEARCH_BENCH_OUTPUT=runs/model-benchmark-repeat/local \
WEB_SEARCH_OMLX_SETTINGS="$HOME/.omlx/settings.json" \
CARGO_HOME="$PWD/.cache/cargo" \
cargo test --locked --offline benchmark::paired_model_benchmark -- --ignored --exact --nocapture
```

Offline-пересчёт текущих сохранённых captures, без модели и без чтения ключей:

```sh
python3 scripts/analyze-model-benchmark-2026-10-30.py
```

Скрипт использует только стандартную библиотеку Python как dev tooling, не участвует в runtime приложения. В долговечной сводке сохранены исходные/guarded/final метки каждого ID, ранги и ошибки; они позволяют проверить матрицы отдельно от сырых provider-ответов. Перед анализом проверяется совпадение corpus с manifest. Полные фактические запросы для побайтовой проверки остаются в диагностических captures.
