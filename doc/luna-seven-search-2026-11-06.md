# Поиск: семь Luna, 6 ноября 2026

По прямому запросу пользователя запущены **семь разных субагентов `gpt-6-luna`**. Они выполнили офлайн-проверки и14 зафиксированных живых эпизодов. Первичные отчёты перепроверены координатором. Найдены **одно неверное фактическое утверждение research и две причины потери правильных ответов**. Runtime не исправлялся и не выпускался.

Проверен продуктовый снимок исходного commit **`82a3756`**, SHA-256 бинарника/исходников — [результаты](../feasibility/luna-seven-results-2026-11-06.json). Он совпадает с проверенным package предыдущего MVP. Внутри продукта остались DeepSeek `deepseek/deepseek-v4.1-flash-1017` и локальный `Qwen3-Reranker-0.6B-4bit`; Luna — модель тестирующих субагентов. Использованы `--evidence-memory`, opt-in cache и общий persistent source-state; это не сравнение с default legacy или локальной генеративной моделью.

## Что проверяли агенты

| Агент | Фактическая проверка | Результат и граница |
| --- | --- | --- |
| L1 security | Существующий stdio discovery/validation на frozen binary | 1 интеграционный тест прошёл; отрицательные параметры/private URLs проверены, не новый полный аудит SSRF |
| L2 extraction | 11 новых заранее сохранённых HTML fixtures, actual `extract-file` | 11CLI executions успешны;9prefix-marker matches,2некорректных/представительных gold-проверки. Все11 без собственных Article metadata, настоящего truncation-кейса не было |
| L3 cache | 3существующих offline cache tests; CLI очистки отдельной foreign SQLite DB | 3/3; чужаяБД отклонена, sentinel table/row сохранены |
| L4 budgets | 11существующих research regressions и CLI без `--live` | 11/11; отказ до чтения отсутствующих settings. Fake-backend tests не доказывают отмену удалённого вычисления |
| L5 retrieval | 3search +warm repeat,3ranked technical queries,1news ranked | 8вызовов завершены; среди20возвращённых ranked-карточек один direct, проверен как подходящий |
| L6 research | 3технических +1новостной research | 4/4partial; содержательно правильный Python answer, неверное нормативное утверждение и потеря take_while |
| L7 Harness | 2настоящих SDK episodes в отдельном profile | 2completed, всего3MCP calls/5outerLLM; оба конечных задания полностью не выполнены, нехватка сообщена честно |

Одновременно допускалось не более3субагентов; живые действия шли через общую файловую очередь. Это не нагрузочный тест семи параллельных поисковиков. Первые поручения L1/L3 закончились статическим просмотром, L2 ошибся аргументами CLI, L5/L6 потребовали дополнительного этапа сохранения результатов. Эти сбои исполнения не посчитаны продуктовыми отказами и не скрыты. Все7 в итоге оставили `results.json`/`report.md` в корпусе.

L5/L6 не выполнили самостоятельный семантический аудит и явно пометили его `not_audited`; содержательные выводы ниже проверял координатор. Это **не семь независимых приёмок**. Исправлена и интерпретация L5: счётчики реально находятся в `cache.hits/http_reads`, поля `source_cache.hit` в ответе нет.

Чтобы закрыть недостаток первых fixtures, координатор отдельно зафиксировал и проверил4component-cases: собственная JSON-LD Article date, исключение чужой даты в teaser/footer, конфликт двух собственных дат и честное усечение110000символов до100000. **4/4** прошли, включая повтор сохранённым replay-скриптом. Это дополнительные проверки координатора; не часть первоначальных11Luna-case и не полнота реальных сайтов.

## Зафиксированная живая серия

Вопросы/gold/references сохранены **до inference** в [cases](../feasibility/luna-seven-cases-2026-11-06.json). Reference primary pages были прочитаны отдельно; URL не инжектировались в продукт. S01W — намеренный повтор S01 для кэша. Одни темы также проверяются разными инструментами:14эпизодов нельзя считать14независимыми quality-вопросами. Запросы исходных20/B17 и исходники runtime остались byte-identical.

| ID | Инструмент/тема | Время без ожидания очереди | Наблюдение |
| --- | --- | ---: | --- |
| S01/S01W | search, Option::or/or_else | 0,962с /0,026с | карточки совпали; HTTP1→0, cache hit1 |
| S02 | search, Python partition | 0,821с | выдача преимущественно про Python вообще |
| S03 | search, HTTP429/Retry-After | 0,789с | показаны SERP-карточки; истинность по snippet не устанавливалась |
| D01 | ranked, Option::or/or_else | 22,937с | C003direct: прочитанный Rust reference подтверждает eager/lazy и условие вызова closure |
| D02 | ranked, Python partition | 1,935с | direct0, даже целевые страницы не прочитаны; retrieval miss |
| D03 | ranked, HTTP429 | 10,626с | direct0; общие HTTP-страницы остаются background |
| N01 | ranked, новости Воронежа6ноября | 18,333с | direct0; неподходящие/старые страницы не стали новостями |
| R01 | research, MUST/MAY для Retry-After | 54,185с | partial,4claims; A1ошибочно приписывает RFC6585словоSHOULD |
| R02 | research, Python partition | 17,100с | оба содержательных аспекта верны; partial из-за одного цитируемого тела вместо2 |
| R03 | research, consumed false-item take_while | 20,852с | правильный фрагмент прочитан и выбран Qwen, но готовых claims0 |
| N02 | research, две новости Воронежа6ноября | 9,653с | claims0, все аспекты missing; чтение неподходящих источников |
| H01 | Harness→research, Retry-After | 18,846с | 1MCP/2outerLLM; не ответил про заголовок, честно перечислил gaps |
| H02 | Harness→search→extract, новости | 5,893с | 2MCP/3outerLLM; прочитал страницу приложения2019г., не выдал её за новость |

Все14эпизодов завершены без runner/model-refusal ошибок. Это означает завершение выполнения, **не14успешных ответов**. Четыре direct-research результата — `partial/budget_exhausted`; полезное полное содержание одного вопроса R02 не отменяет невыполненное формальное условие двух цитируемых документов.

В4ranked-вызовах возвращено20карточек: **1проверенный direct,0выявленных false-direct,0unknown-direct**. Это один положительный пример, а не доказательство безопасности или измеренный web recall. У research другая единица:6выпущенных утверждений в4вызовах, из них5соответствуют проверенным reference facts,1ошибочное. Эти числа и повторы не объединяются с Harness/историческими наборами.

## Три конкретных проблемы

1. **R01/A1: точная цитата ошибочного источника стала ложным фактом.** Ответ приписал RFC6585 рекомендациюSHOULD для Retry-After. В [RFC6585§4](https://www.rfc-editor.org/rfc/rfc6585.html#section-4) включение заголовка — MAY; SHOULD относится к пояснению причины ограничения. Вторичный errorlookup.com содержит ошибочную формулировку, а отдельный модельный audit её принял. Provenance верен, нормативная атрибуция неверна. `partial` не гарантирует, что каждый выпущенный факт истинен.
2. **R03: одна плохая дополнительная цитата уничтожает пригодное утверждение.** Источник Rust прямо объясняет потребление false-item; абзац сохранён в `S1P2` и виден draft-модели. Вторая цитата из source-code страницы скопирована неточно. `prune_invalid_claims` отбрасывает весь claim, хотя первая citation полностью точная. Это потеря после успешного extraction/Qwen, а не нехватка свидетельств. [Официальное описание](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.take_while).
3. **R03/draft2/claim3: квадратные скобки запрещают нормальный технический пример.** Текст с массивами и результатомcollect содержит `[]`; обе цитаты точные. Общий запрет Markdown/HTML в `validate_draft` отвергает этот текст как `invalid/duplicate answer text`. Наблюдение не требует предположения о новом модельном ответе: причина отказа и точные witnesses уже сохранены.

Точные IDs, supporting data и границы — [misses](../feasibility/luna-seven-misses-2026-11-06.json). Runtime-исправлений здесь нет. Следующие отдельные задачи: требовать первичную опору для нормативной атрибуции, проверять/удерживать citations по одной перед повторным semantic audit и отличать опасную Markdown-ссылку от массива/кода. Простое ослабление truth/quote gates не обосновано.

## Проверки происхождения, Harness и кэша

Проверены **7исходных Unicode witness placements и7answer ranges** в direct-research/H01; отдельно **5нормализованных UTF-8 byte ranges** ranked, включая4body witnesses и1context-only title. Все срезы и хэши совпадают. Разные системы смещений не смешивались. R01/A1 показывает, почему совпадение срезов не считается истинностью.

Для H01 фактический tool-content из следующего outerLLM payload полностью равен публичному JSON внутреннего research. Для H02 текст извлечённой страницы, реально увиденный агентом, совпадает с `text_hash`. Tool-output spill files не обнаружены; это проверка двух реальных эпизодов, не всех клиентов и больших ответов. Текущий Desktop Harness profile не проверялся и не редактировался.

Повтор S01W — один ограниченный warm-cache control. Получены те же карточки, cache.hits1 и http_reads0; это не новая живая HTTP-проверка. Результат0,962→0,026с нельзя переносить на среднюю задержку всего поиска/LLM. Отказы источников проверяются перед cache hit; общий ledger после серии совпадает с начальным. Прежние отказы Naver/Bing/DDG/Mwmbl и5target-hosts, включаяredis.io/www.mujrozhlas.cz, не очищались и не повторялись. Ротации прокси/отпечатков/новых search API не было.

Observed source HTTP attempts — **48**, local Qwen HTTP calls — **23**. В25внутренних и5outerHarness генеративных ответах usage.cost известен: **USD0,002885450 +0,000565992 =0,003451442**. Это API-расход поиска/Harness; native Luna agents, локальное железо/электричество и metadata-request overhead этой суммой не оценены. Контексты генерации до19439символов при лимите24000; budgets/deadlines не превышены.

После live добавлена offline-проверка registry/caps/dispose тестового Harness guard, а detection model-refusal уточнён для настоящего поля error и preflight401/403/429. Refusal в наблюдавшихся14эпизодах не было; runtime и записанные live результаты не изменены, дополнительных live повторов нет.

## Проверенный снимок, приёмка и выпуск

- Product implementation: исходный82a3756, полный Git ID и SHA-256 бинарника/исходников в [results](../feasibility/luna-seven-results-2026-11-06.json). Новый diff содержит тестовый driver/guard, минимальное расширение старого Harness helper двумя optional параметрами и проверяемые отчёты.
- Проверки: **137unit +6stdio**, отдельно6stdio на frozen release, fmt; driver self-check, Python compile, Node syntax/guard assertions. L1/L3/L4 — подмножества общей suite, их числа не прибавляются к137+6.
- Приёмка: воспроизводимость и перечисленные проверки подтверждены исполнителем/координатором. `ready_to_merge` независимым reviewer и общая двухдневная qualification не проводились. Отчёты Luna этого не заменяют.
- Выпуск: нового product/release commit или deploy нет. Отдельный task commit фиксирует только тестирование и доказательства; runtime-snapshot остался прежним.

[Cases](../feasibility/luna-seven-cases-2026-11-06.json), [results](../feasibility/luna-seven-results-2026-11-06.json), [misses](../feasibility/luna-seven-misses-2026-11-06.json), [raw corpus](../feasibility/luna-seven-corpus-2026-11-06.tar.gz), [package](../feasibility/luna-seven-package-2026-11-06.json); корпус исключён из Git и учтён в [artifact manifest](../feasibility/artifact-manifest.json). Ключи, SDK homes/configs, binaries/cache/vendor не упакованы. `runs` не является единственным долговечным источником результатов.

Не проверены полная adversarial/нагрузочная серия, свежий pairwise recall benchmark, другая генеративная модель, VPS/publicJS/browser и все client cancellation paths. Новости Воронежа в этой выборке не найдены; отсутствие таких новостей в интернете не установлено.
