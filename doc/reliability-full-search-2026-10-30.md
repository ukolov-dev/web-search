# Начальный classifier, ложные direct и полный поиск с Qwen3

Дата: 30.10.2026. Задача `TASK-20261030-164949-001`. Это собственная инженерная проверка, не независимая приёмка пилота.

Финальная версия восстановила завершение начальной классификации и добавила обязательную смысловую проверку всех direct в режиме `--evidence-memory`. На свежих **8 запросах / 154 карточках** полный pipeline с локальным Qwen3-Reranker получил **direct recall 6,25% → 56,25%, ложные direct 0 → 0**. Это измерение относительно 16 известных релевантных источников. Нулевой FP на девяти direct не доказывает отсутствие ошибок вообще. Default legacy сохранён; семантическая проверка v4 включается явно.

## Что исправлено

### Завершение initial classifier

У закреплённого `deepseek/deepseek-v4.1-flash-1017` скрытое reasoning расходовало выходной бюджет. В старом диагностическом12 было **6 `finish_reason=length` и 1 `stop` с пустым видимым content**, а не семь length: это уточняет [прошлый отчёт](reranker-2026-10-30.md). Пустой `stop` также не является корректной классификацией.

Для этой точной модели initial теперь получает `reasoning.effort=none`, `max_tokens=4096` вместо 2400. Покрытие evidence получает 6000, audit — 2400. У остальных моделей initial сохраняет прежние 2400 без неподтверждённого override. Проверки всех ID, schema, уникальности ключей, `finish_reason`, предупреждений и происхождения цитат сохранены; обрезанный JSON не ремонтируется и не принимается частично. Автоповторов нет.

На семи прежних ошибочных входах повтор завершился **7/7 корректно, reasoning tokens=0**. Старый код в отдельном воспроизведении снова упал на length, но один прежний ошибочный случай прошёл: исходный сбой зависел от генерации. Поэтому результат не называется детерминированной гарантией. Механизм бюджета соответствует [официальной документации OpenRouter](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens); отключение reasoning дополнительно проверено фактическим usage ответов, а не только документацией.

### Связь предмета, утверждения и условия

`request-evidence-v4-audited` использует существующий skeptical audit для **каждого** кандидата, которому coverage предлагает direct, включая исходные SERP-direct. Одного совпадения слов или дословной цитаты недостаточно. Проверяется связь SUBJECT–PREDICATE–CONDITION; правило соседнего HTTP-header нельзя переносить на If-Range. Отрицательный ответ на ошибочную предпосылку вопроса может быть релевантен.

Порядок внутри прежнего бюджета: initial classifier → чтение до четырёх кандидатов и выбор Qwen до трёх фрагментов на документ → один coverage → один skeptical audit. Максимум остаётся **3 генерации / 4 попытки чтения / 180 секунд** после preflight, дополнительно до4 локальных rerank calls внутри того же deadline. При нескольких initial batches остаточного бюджета может не хватить для этого пути; непроверенные direct не повышаются.

Coverage задаёт предмет и до3 якорей из ограниченного набора настоящих уникальных подстрок запроса; R0 всегда содержит исходный запрос. Для каждого supported-требования нужна **непрерывная цитата 16–240 символов** из указанного source passage. Core нельзя подтвердить одним заголовком. Rust проверяет ID, границы, hashes и точное вхождение цитаты. Некорректная цитата локально становится ambiguous с причиной в `witness_rejections`; неизвестные ID/ссылки, изменение контракта и некорректная schema по-прежнему отклоняют batch целиком.

Audit рассматривает совокупность цитат, их контекст и настоящий HTML page title. Цитата не обязана каждый раз повторять имя API. Literal guard использует реально процитированные supplied passages. Для `module.name` разрешено короткое `name` лишь при соответствующем namespace в начале настоящего page title с границей имени; версия вроде `3.12` так не сокращается. Есть положительный и отрицательный wrong-module тесты. Это привязка контекста, а не доверие к репутации URL.

Непринятый audit означает background. Public proof содержит отдельные точные цитаты с UTF-8 ranges/hash; контекстный page title помечен `context_only`. Память остаётся внутри запроса. Reviewer и critic — **одна модель с разными инструкциями**, они могут ошибаться согласованно; это не независимый verifier и не проверка истинности готового ответа.

На прежних трёх ложных If-Range direct реальный повтор с v4 дал **0/3 direct**. Отдельный синтетический regression test воспроизвёл повышение по правилу соседнего API до исправления и проходит после него. [Native red/green report](../outputs/classifier-relation-2026-10-30/bug-reproducer-report.md) относится к этим узким дефектам, а не к безошибочности всего поиска.

## Неудачные промежуточные варианты сохранены

Первый новый набор из12 вопросов/228 карточек стал development после анализа. Слишком строгая v3 требовала повторять subject в каждой цитате и лишала контекста literal guard. Получилось: baseline **TP6/FP5/FN10, recall37,5%**, v3 **TP0/FP1/FN16, recall0%**. Такой вариант отвергнут. V4 проверяет смысловую совокупность свидетельств; короткие development-повторы вернули полезные direct без новых FP в этих повторах. Эти повторы не включены в финальную оценку.

Также первый технический запуск под именем baseline оказался собран новым кодом из-за одинакового имени Cargo package в общей target-директории. Несоответствие нашли по manifest до разбора качества. Он исключён, исходные результаты сохранены. Изолированный baseline получил уникальное package name; ранняя проверка версии executable выполняется до новых генераций. Стоимость ошибочного запуска **30 генераций / $0,004939846** включена в общий учёт, но не в A/B.

[Development corpus](../feasibility/classifier-relation-corpus-2026-10-30.json), [его gold](../feasibility/classifier-relation-gold-v1.json), [неудачный v3 результат](../feasibility/classifier-relation-results-2026-10-30.json) остаются историческими артефактами. Их нельзя выдавать за результат финальной v4.

## Протокол свежего полного A/B

После доработки зафиксированы финальные исходники, отдельные **8 новых вопросов**, исходные SERP и их hashes. После запуска по этому набору код больше не дорабатывался. Baseline — прежние `conservative-evidence-v3-body` / `request-evidence-v2`; fixed — `conservative-evidence-v4-bounded-output` / `request-evidence-v4-audited`. Оба используют ту же DeepSeek-модель и локальный **Qwen3-Reranker-0.6B-4bit через oMLX0.7.0**. Initial не обходится и не подменяется заранее заданными метками.

Seznam/Naver реально опрошены один раз:16 source calls, все200,154 дедуплицированные карточки. Обе ветки получают идентичные candidates/sources, initial messages/schema. Меняются модельный бюджет/reasoning и последующий evidence-путь. Это полный поиск на замороженном общем live retrieval, а не два независимых повторных обхода поисковиков. Новые формы adaptive expansion не потребовались.

Чтение страниц проходит обычные решения и лимиты ветки. URL snapshots успешных и неуспешных чтений общие; первая попытка читает сеть, последующая использует тот же результат. Необходимость прочитать другой документ — часть различий pipeline. Блокировки хостов сохраняются, обходов CAPTCHA, смены маршрутов страниц и повторов заблокированных URL нет. Всего32 уникальных URL outcomes,23 успешных извлечения;25 HTTP200 включают два провала extraction,4 timeout,2 blocked,1 unsupported content. Эти числа не равны количеству сетевых пакетов или redirect hops.

Gold размечен одним автором **до просмотра предсказаний, но после inference**. Размечены25/154 карточек:16 positive,9 partial/irrelevant,129 unjudged. Источники — объединение retained snapshots и два отдельные чтения primary docs только для gold (Docker/MDN). В `git-clean` описание флагов проверено в уже сохранённом HTML: extraction потерял его, хотя исходная страница релевантна. Эти дополнительные сведения в модельные ветки не передавались. Недоступный или короткий источник без достаточного свидетельства не объявлялся нерелевантным. Unjudged direct считаются отдельно. Это recall относительно известных релевантных retrieved sources, **не полнота поиска по всему вебу**. Разметка не является независимой приёмкой.

## Результаты

| Метрика | Baseline + Qwen | V4 + Qwen |
| --- | ---: | ---: |
| Корректный initial | 2/8 | **8/8** |
| Initial length / stop-empty | 5 / 1 | **0 / 0** |
| Initial reasoning tokens | 17337 | **0** |
| True direct / false direct / FN | 1 / 0 / 15 | **9 / 0 / 7** |
| Unjudged direct | 0 | 0 |
| Direct recall | 6,25% | **56,25%** |
| Precision среди размеченных direct | 1/1 | 9/9 |
| Релевантные в returned top10, любой класс | 14/16 (87,5%) | **15/16 (93,75%)** |
| Генерации | 12 | 22 |
| Логические target read attempts | 7 | 30 |
| Snapshot cache hits | 0 | 5 |
| Local rerank calls / pairs | 5 / 154 | 22 / 306 |
| Reranker fallback | 0 | 0 |
| Source proof ranges проверены | 1 | 45 |
| Initial median latency | 25,118 с | 2,557 с |
| Full replay median latency | 32,577 с | 16,889 с |
| Reported OpenRouter cost | $0,004497710 | $0,003802792 |

Все34 генерации fresh A/B обслужил Morph. Cost взят из usage, это не отдельная сверка биллинга. Timing описывает последовательный replay: сеть страниц и provider/snapshot cache различаются; это не изолированный latency benchmark и не SLA. Рост recall на50п.п.; descriptive query bootstrap95%: +23,1…+75,0п.п. На восьми вопросах такой интервал не заменяет большой независимый benchmark.

| Запрос | Известных positive | TP baseline → v4 | FP baseline → v4 |
| --- | ---: | ---: | ---: |
| HTTP202: гарантирует ли завершение | 3 | 0 → 1 | 0 → 0 |
| PostgreSQL sequence: rollback/cache/gaps | 2 | 0 → 1 | 0 → 0 |
| Python zip strict: ошибка при потреблении | 3 | 0 → 3 | 0 → 0 |
| Rust fs::rename: filesystem/replacement | 1 | 1 → 1 | 0 → 0 |
| Docker on-failure/unless-stopped/daemon | 4 | 0 → 2 | 0 → 0 |
| SQLite AUTOINCREMENT: reuse/overhead | 1 | 0 → 1 | 0 → 0 |
| git clean -n/-x/-X/directories | 1 | 0 → 0 | 0 → 0 |
| CSS hidden/none: layout/accessibility | 1 | 0 → 0 | 0 → 0 |

На новом наборе baseline не дал ложных direct во многом потому, что initial сорвался на6/8 вопросах. Поэтому здесь доказано восстановление полезных direct **без наблюдаемого роста FP**; снижение false direct показано отдельно историческим If-Range контролем и development12, а не выдуманным улучшением0→0.

## Что ещё теряется

- HTTP202: MDN и короткий корейский источник остались background из-за непокрытых выбранных coverage anchors. Это избыточная строгость модели, а не нехватка памяти.
- PostgreSQL: wiki прошла; второй источник не покрыл один coverage anchor.
- Docker: официальный документ присутствует в пуле, но не прочитан внутри четырёх попыток; другой подробный источник потерял буквальную привязку `on-failure` в выбранных фрагментах.
- `git-clean`: source HTML содержит нужные флаги, extractor оставил Interactive mode. В coverage дополнительно возник `title-only core`, и строгая проверка отклонила весь batch. Initial при этом корректен. Защиту не ослабляли ради метрики.
- CSS: нужная MDN-страница не попала в отобранные10; выбранные статьи помогают layout, но не закрывают accessibility. Единственный известный positive отсутствует в returned top10.

Полнота extraction и ранний отбор кандидатов остаются самостоятельными ограничениями. Reranker выбирает фрагменты только из извлечённого body и не может восстановить удалённый раздел, непрочитанный документ или отсутствующего кандидата. Порог score и дополнительная долговременная память в этой работе не менялись. Следующий эксперимент стоит направить на сохранение секций reference docs и выбор четырёх документов с учётом непокрытых условий; этот набор после публикации становится regression/development, а не новым holdout для настройки.

## Проверки и границы

Прошли56 обычных тестов (51unit+5stdio), `cargo fmt --check`, all-targets clippy `-D warnings`, release build и5 stdio тестов свежего release. Отдельно по имени выполнены live initial replay, исторический relation replay, source capture/gold audit и обе ветки полного поиска. Весь `--ignored` не запускался.

Дополнительный **реальный release MCP** запрос на известный здоровый MDN-путь дал `status=ok`, initial valid,3 генерации/4 чтения/**4 Qwen calls/61pairs**, без errors. Он проверяет опубликованный runtime без test-only snapshot reader и контракт всех direct с audit; это операционный smoke, не девятый вопрос quality A/B.

Анализатор проверил идентичность initial inputs, отсутствие bypass/gold в модели, лимиты, версии разных executable, все45 source proof ranges/hash, неизменность code/query/gold freeze. Original20 не менялись: SHA-256 `b67e3269830ae0e976aa9b4c5dc2f6b9c3e922ad1adda35738289d6b1b8325d6`. B17 forms, публичные URL/DNS/IP/redirect/site ограничения сохранены. Test-only replay не входит в release. Собственный diff review выполнен; Router refactor heuristic без Git не может оценить diff и не считается семантическим review.

Основной A/B стоил $0,008300502. Все сохранённые этапы этой задачи, включая проигравшийv3, development и ошибочный executable: **171 уникальная priced generation / $0,032942672**; response IDs дедуплицированы. Дополнительно **4 unpriced generations**: одна ранняя development-ошибка quote contract без сохранённого response и три release MCP вызова без выгруженного usage. Их стоимость неизвестна, не нулевая. Локальный scoring не включён в provider cost. [Учёт исполнения](../feasibility/reliability-execution-accounting-2026-10-30.json).

Context lint/index freshness/smoke прошли. Doctor:15ok/5warn/0fail; предупреждения относятся к отсутствию Git/проверки tracking, широкому coverage tooling и отсутствию ready backlog. Они не являются подтверждением сохранности в Git. Verification evidence и last_result сохранены в Router; формальное completion partial, поскольку независимой ready_to_merge приёмки нет.

MiMo с v4 по качеству, повтор в другой день, нагрузка, VPS/API, модель другого провайдера и независимая приёмка не проверялись. Нельзя переносить измерения pinned DeepSeek+Morph на все OpenRouter-модели. Default legacy имеет прежнюю семантику и не получает обязательный v4 audit без startup-флага.

## Артефакты и повторяемость

- [Свежие8 вопросов](../feasibility/reliability-holdout-cases-v1.json), [frozen source gold](../feasibility/reliability-holdout-gold-v1.json), [метрики/per-case](../feasibility/reliability-holdout-results-2026-10-30.json).
- [Долговечный corpus](../feasibility/reliability-holdout-corpus-2026-10-30.json): source pool, snapshots, обе ветки с model inputs/outputs, manifests и freeze hashes. Не содержит ключей.
- [Blind gold builder](../scripts/reliability-holdout-gold-2026-10-30.py) отказывается перезаписывать уже существующий gold. После раскрытия labels gold не регенерировать.
- [Анализатор](../scripts/analyze-classifier-relation-2026-10-30.py): `python3 scripts/analyze-classifier-relation-2026-10-30.py --holdout`. Он работает offline на durable corpus.
- Raw logs/source snapshots/раздельные executable manifests находятся в `runs/classifier-relation-2026-10-30` и `runs/reliability-holdout-2026-10-30`; runs не являются долговечной памятью проекта.

Живые пробы — только по имени: `reliability_benchmark::{initial_completion_replay,capture_new_search_pool,full_pipeline_replay,audit_new_gold,old_false_direct_replay}` и integration `live_audited_reranker_pipeline`. Новое получение источников зависит от текущей доступности; не повторять ранее заблокированные URL ради воспроизведения старой цифры. Startup остаётся [описанным в MCP-дополнении](local-mcp.md): `--evidence-memory --reranker-settings <local settings>` с явно настроенным classifier provider.
