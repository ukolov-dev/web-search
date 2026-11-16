# Qwen: сохранение начала статьи и проверка с Firecrawl, 5 ноября 2026

**Потеря начала статьи исправлена в общем Rust passage selector.** Новостной запрос сохраняет lead в прежнем бюджете трёх body-окон: самый сильный фрагмент Qwen и второй diverse slot остаются, третий при необходимости заменяется первым окном текста. На12других URL с локальным Firecrawl сохранность заранее выбранных фрагментов выросла **14/19→17/19**, на пригодных страницах — **12/16→14/16**. **Direct recall не вырос:8/8→8/8; ложные direct0→0, новых FP IDs нет.**

Эти числа относятся к отдельному повтору с согласованными историческими часами. Первый прогон7/8→7/8, ошибка его протокола и старый диагностический пример сохранены отдельно. Набор source-aware и исторический; это не свежий web holdout, полный поиск или независимая qualification.

## Что изменилось

Ранее `top3_diverse` сохранял strongest Qwen window, затем добирал два окна из top6 по новым lexical query features. В прежнем Firecrawl P01 нужное указание города имело низкий Qwen score и не прошло отбор; полный извлечённый текст его содержал. Форма запроса «Нижнего Новгорода» также не совпадала дословно с «Нижний Новгород-Стригино», поэтому lexical diversity не спасла начало статьи.

Теперь после прежнего operation-context postprocess selector **только для news queries** сохраняет первое окно body, если его ещё нет в выборе. Используется существующий `news::is_query`, включая исключения для download/app selection и tutorials. Технический алгоритм не менялся. Новая метка trace — `query-feature-cover-v3-news-lead`; `candidate_pool` остаётся preview Qwen top6, `pool_ranges` — полным scored pool. Lead может находиться за пределами top6.

Пределы сохранены: до3окон, до1200 Unicode code points каждое, UTF-8 offsets исходного whitespace-normalized текста, strongest Qwen window без переписывания. Новых scoring/generative calls, зависимостей, флагов CLI/MCP и настроек клиента нет. Все source/date/typed Article/literal/status/conflict/coverage/audit gates остались прежними, активный missing блокирует direct. Изменение действует в уже opt-in связке `--evidence-memory --reranker-settings`; default legacy и основной reader не менялись. Firecrawl остался test-only reader adapter.

## Отдельный повтор с правильными часами

| Метрика | Текущий selector v2 | Исправленный v3 |
| --- | ---: | ---: |
| Firecrawl body сохраняет контрольные фрагменты | 21/21 | 21/21 |
| Фрагменты после Qwen, на10допущенных страницах | 14/19 | 17/19 |
| Фрагменты после Qwen, только8пригодных страниц | 12/16 | 14/16 |
| Final direct TP / positive source gold | 8/8 | 8/8 |
| Direct recall | 100% | 100% |
| FN / FP / unknown direct | 0 / 0 / 0 | 0 / 0 / 0 |
| Новые FP IDs / потерянные TP IDs | — | [] / [] |
| Собственные даты исходных статей | 10/11 | 10/11 |
| Generative-вызовы | 8 | 8 |
| Qwen-вызовы / оценённые query–passage пары | 10 / 71 | 10 / 71 |
| Qwen fallback / ошибки групп | 0 / 0 | 0 / 0 |
| Qwen/coverage/audit stage p50, с | 13,776 | 13,105 |
| Та же stage p95, с | 18,760 | 18,424 |

Все10пар Qwen score vectors совпали побитно по JSON-значениям `(index,relevance_score)`. Strongest slot сохранён во всех случаях. Реально изменился отбор уP02,P05,P12; контрольные цитаты не потеряны ни на одной странице. Две восстановленные цитаты относятся к положительнымP02/P05, третья — к отрицательномуP12, который остался background: наличие фрагмента не стало автоматическим direct.

Наличие заранее выбранной цитаты — отдельная метрика от semantic coverage. УP04 иP05 остаётся по одной контрольной цитате вне выбранных окон; эти страницы всё равно direct по исходному новостному запросу. Мы не заявляем полную сохранность всех деталей body. Два отрицательных контроля отсеяны до Qwen по date/type; два других дошли до Qwen/coverage и не получили direct.

Own dates одинаковы в обеих ветках, поскольку получены тем же Rust gate из hash-matching returned rawHtml. УP03 майская дата из source gold не распознана (`published_date=null`), страница не допущена; лентаP09 имеет date N/A. Это не улучшение распознавания дат и не проверка Firecrawl `publishedTime`.

## Все12страниц

Слева v2, справа v3. Gold2 — пригодный источник,0 — неподходящий,1 — фон/лента. «Не допущена» означает, что до passage selection дело не дошло, и эта строка не входит в знаменатель19.

| Страница | Gold | Сохранность выбранных цитат | Final direct |
| --- | ---: | --- | --- |
| P01 [Благоустройство сквера в Перми](https://59.ru/text/gorod/2026/11/03/76741257/) | 2 | 2/2 → 2/2 | да → да |
| P02 [Приговор псевдоюристке в Перми](https://59.ru/text/criminal/2026/11/03/76741420/) | 2 | 1/2 → 2/2 | да → да |
| P03 [Пермский промышленный longread за май](https://59.ru/text/longread/2026/05/21/76426099/) | 0 | не допущена → не допущена | нет → нет |
| P04 [Закрытие рынка в Уфе](https://ufa1.ru/text/business/2026/11/03/76740975/) | 2 | 1/2 → 1/2 | да → да |
| P05 [Депутаты Горсовета Уфы](https://ufa1.ru/text/gorod/2026/11/02/76720417/) | 2 | 0/2 → 1/2 | да → да |
| P06 [Общероссийский дайджест на UFA1](https://ufa1.ru/text/world/2026/11/02/76740698/) | 0 | 2/2 → 2/2 | нет → нет |
| P07 [Видео ДТП в Екатеринбурге](https://www.e1.ru/text/incidents/2026/11/03/76741416/) | 2 | 2/2 → 2/2 | да → да |
| P08 [Продажа памятника в Екатеринбурге](https://www.e1.ru/text/realty/2026/11/03/76741060/) | 2 | 2/2 → 2/2 | да → да |
| P09 [Главная E1: лента](https://www.e1.ru/) | 1 | не допущена → не допущена | нет → нет |
| P10 [Новый театр в Омске](https://superomsk.ru/news/167842-v_omske_rastet_chislo_ofitsialnykh_teatrov/) | 2 | 2/2 → 2/2 | да → да |
| P11 [Зимние авиарейсы из Омска](https://superomsk.ru/news/167820-nazvany_daty_reisov_iz_omska_v_krasnodar_na_zimnie_kanikuly/) | 2 | 2/2 → 2/2 | да → да |
| P12 [Роботы-доставщики: без связи с Омском](https://www1.ru/articles/2026/11/03/ii-razognal-rovery-iandeksa-na-43-kak-ustroeny-roboty-dostavshhiki-i-kto-reguliruet-ix-dvizenie.html) | 0 | 0/1 → 1/1 | нет → нет |

Группы:Q01 «новости Перми»,Q02 «последние новости Уфы»,Q03 «новости Екатеринбурга»,Q04 «новости Омска». Каждый source URL отличается от всех12URL [предыдущего Firecrawl A/B](firecrawl-benchmark-2026-11-05.md). HTML и прежняя source-разметка — из сбора3ноября; publishers и часть запросов известны из прежней разработки. Это URL-disjoint относительно прошлого Firecrawl12, а не полностью неизвестные проекту страницы.

Точные21цитата, исходные HTML, source hashes/variants и причины gold находятся в архиве. Нормализуются только пробелы; включение цитаты проверяется по точному тексту. Analyzer сверяет реальные B-text в первом coverage request с UTF-8 ranges и хешем нормализованного body, а не только с заявленными trace ranges.

## Ошибка часов в первом прогоне

Первоначально был перенесён clock из прежнего опыта: `2026-11-03T12:48:44.707451Z`. Он раньше собственных публикацийP10 (14:00Z) иP12 (14:06Z). При calendar-day gold «1–3ноября» P10 заранее размечен как пригодный, но точный timestamp делает его будущим. Date gate правильно вернул `future_publication`; селектор не видел эту страницу. Это **ошибка протокола оценки**, а не потеря Qwen или defect date guard.

Первый frozen результат не переписан:

| Первый прогон | v2 | v3 |
| --- | ---: | ---: |
| Direct по заранее объявленному source gold | 7/8 | 7/8 |
| FP / unknown direct | 0 / 0 | 0 / 0 |
| Фрагменты на8допущенных страницах | 12/16 | 14/16 |
| Stage p50 / p95, с | 11,850 / 17,479 | 9,492 / 30,193 |

Перед повтором заморожено отдельное clock amendment. Единственное изменение inputs — `as_of_timestamp`: время завершения **того исходного сбора**, `2026-11-03T14:13:52.222315Z`, взятое из прежнего `native-source-freeze.json`. В нём подтверждены hashes всех12точных raw HTML. Дата/интервал1–3ноября, gold, URLs, HTML, оба бинарника, модельные параметры и selector не менялись. Повтор всех4пар выполнялся в отдельном каталоге; primary/repeat не объединяются. Повтор адаптивный из-за исправления протокола и не выдаётся за новую слепую проверку. Дополнительные16generations отражены в бюджете, новых Firecrawl/target reads не было.

## Прежний P01: доказательство исправления

Frozen-score regression воспроизводит реальные старые Qwen ranges. До изменения unit test падает на точной source-цитате «В пути следования предусмотрены остановки в том числе на станциях Нижний Новгород-Стригино, Владимир.». После изменения проходит без model calls; source text и scores неизменны.

Старые ranges: `[7100,9291]`, `[3578,5661]`, `[1812,3905]`. Новые: `[7100,9291]`, `[3578,5661]`, `[0,2185]`. Это UTF-8 bytes, а не номера символов. Strongest и второй slot не изменились; нужная цитата города появилась в фактически supplied coverage context.

Отдельная живая диагностика на прежних3страницах F01 подтвердила city witness и supported core+city у исправленного варианта; audit принял P01 и P02. Однако новый baseline model pass завершился `audit reason length`: исходный ответ/ошибка сохранены, повторов для получения зелёного результата не делали. Поэтому **paired final-direct прирост по этому старому примеру не засчитывается**. Он не входит в NEW12, а тест доказывает восстановление конкретного фрагмента.

## Задержка и фактический расход

Общий Firecrawl reader измерен3раза на каждой из12страниц:36local uploads, без archive I/O внутри таймера. **p50=171,257мс, p95=230,670мс**,36наблюдений. Обе selector-ветки получают один и тот же Firecrawl r0 документ; повторных reader calls на ветку нет. Эти числа нельзя напрямую сравнивать с32мс предыдущей выборки: страницы и HTML другие.

Stage time включает загрузку сохранённых документов, Qwen, coverage и audit на тройку кандидатов; source fetch/Firecrawl upload в неё не включены. На ветку всего4групповых наблюдения, p95 nearest-rank равен максимуму. Незначительное снижение13,776→13,105с в clock-aligned repeat **не доказывает ускорение**: число запросов/пар одинаково, LLM latency шумная, а первичный прогон имел противоположное изменение хвоста до30,193с. Отдельного CPU microbenchmark selector не выполняли; первоначальный план такого замера сохранён и уточнён до quality calls, чтобы использовать имеющуюся инструментализацию.

Всего в задаче: **37локальных Firecrawl uploads** (1synthetic smoke+36capture),36дополнительных локальных Rust extract как контроль существующего capture runner, **36OpenRouter generations**, **42local Qwen calls /302пары**. Из них primary16gen/16Qwen/114pairs, clock-aligned16/20/142, old diagnostic4/6/46. У всех36generation response model=`deepseek/deepseek-v4.1-flash-1017`, finish=`stop`; у всех42Qwen model=`Qwen3-Reranker-0.6B-4bit`, fallback0. Старый audit schema error всё равно считается ошибкой, несмотря на finish stop.

Search/target HTTP/browser loads — **0**. Новых LLM providers или Firecrawl Cloud API нет. Per query-arm: до2generative,3Qwen,3предоставленных страниц,180с и24000source chars; продуктовые пределы3generative/4reads/4Qwen не увеличены. Внешняя модель получает публичные фрагменты через уже разрешённый OpenRouter; ключи/настройки не сохранены в captures.

## Реализация, изоляция и проверки

Использован pinned upstream Firecrawl `BUILD_SHA=7d8e92dc5d270b1247d60e809c3fd32f52e0e47b`, official manifest `sha256:d09e210a9fbbf1fffb6ec8972f23e854498cdee672ee374dbdce9c344a301d8e`. API+Redis повторно запущены из имеющегося image cache; `/v2/parse` получает оригинальный UTF-8 HTML upload, rawHtml обязан совпадать поSHA, cleaned HTML сериализуется существующим Rust formatter. Синтетический upload URL не выдаётся за живую source/redirect provenance.

Только собственный temporary Compose project: internal network без внешнего default route, без host ports, API2CPU/2GiB, Redis0,5CPU/128MiB, cap_drop/no-new-privileges, no browser/cloud keys. API memory peak **449,10MiB** в этом sequential parse режиме; полный JS stack/VPS load не измерялся. Контейнеры и сеть удалены после опыта; все8прежних контейнеров работают, их отсутствие рестартов/функциональная приёмка не заявляются. Prune и глобальные изменения не выполнялись.

Baseline и candidate — два сохранённых Rust test executable, построенные до нового quality inference. Shared harness вызывает реальные `reranker::select`, `evidence::run_with_reader`, coverage/audit/finish и news enforcement; Firecrawl reader одинаковый. Initial labels ambiguous/background, нет retrieval/initial classifier. Порядок пар чередовался:Q01/Q03 baseline→fixed, Q02/Q04 fixed→baseline. Новые env controls существуют только в test harness, MCP schema и trusted startup не расширялись.

Проверки:

- Red→green старого source-bound fixture; bounded slots/UTF-8/strongest-window tests, исключения download/tutorial, прежний реальный operation-return boundary test.
- `cargo fmt --check`, locked/offline **118unit+5stdio=123passed**, clippy all-targets `-D warnings`, release build, ещё5stdio-тестов свежего release процесса.
- Замороженные raw quality outputs/gold/HTML и код после freeze неизменны; Cargo.toml/Cargo.lock, original20/B17, unrelated runtime functions не менялись. Из production logic изменён только selector и его trace identifier.
- Прямой второй пересчёт raw labels и sets подтверждает результаты; это собственная сверка, не независимая acceptance.

Router refactor-review выполнен на конкретных файлах, но из-за отсутствия Git вернул пустой diff; фактический source diff сохранён и прочитан. В task `TASK-20261105-115349-001` сохраняются verification и итог. Независимый accepted `ready_to_merge` не получен; формальный completion partial не означает ожидание разрешения пользователя или незавершённые измерения этой серии.

Context lint/index/index-check и реальный Router stdio smoke прошли; ссылки отчёта проверены. Doctor: WARN,15OK/5WARN/0FAIL — непроверенные Git ignore/tracking, кандидаты вне source globs и отсутствие ready/open backlog. Схемы, активные ссылки и индекс исправны; Git/globs/hooks ради чистого doctor не менялись. Finding сохранён в research revision27.

## Артефакты и офлайн-пересчёт

- [Первый прогон с ошибкой исторических часов](../feasibility/qwen-firecrawl-results-2026-11-05.json).
- [Отдельный clock-aligned repeat](../feasibility/qwen-firecrawl-clock-aligned-results-2026-11-05.json): все per-page witnesses, даты, ranges, final labels и latency.
- [Execution summary](../feasibility/qwen-firecrawl-execution-2026-11-05.json) и [корпус сырых captures](../feasibility/qwen-firecrawl-corpus-2026-11-05.json): sourceHTML/gold/freezes, оба clock cohorts, все Qwen/LLM requests/responses, old diagnostic со schema error, baseline/fixed source и code hashes, установка/teardown/checks. Бинарники не включены; ихSHAсохранены.
- [Selector](../src/reranker.rs), [старый P01 fixture](../tests/fixtures/reranker/news-lead-firecrawl.json), [общий test harness](../src/firecrawl_benchmark.rs), [подготовка](../scripts/prepare-qwen-firecrawl-2026-11-05.py), [analyzer](../scripts/analyze-qwen-firecrawl-2026-11-05.py).

Пересчёт без Docker, ключей и inference в **новый** каталог:

```sh
python3 scripts/restore-firecrawl-2026-11-05.py runs/qwen-offline-review --corpus feasibility/qwen-firecrawl-corpus-2026-11-05.json
python3 scripts/analyze-qwen-firecrawl-2026-11-05.py --run-dir runs/qwen-offline-review --output runs/qwen-offline-review/primary.json
python3 scripts/analyze-qwen-firecrawl-2026-11-05.py --run-dir runs/qwen-offline-review/clock-aligned --output runs/qwen-offline-review/aligned.json
```

Оба итоговых JSON должны побайтово совпасть с соответствующими файлами вfeasibility. Старый однопозиционный вызов restore для прошлого Firecrawl corpus сохранён; `--corpus` только выбирает архив, ничего не запускает.

Исправление оставлено в opt-in Qwen news path. Доказан рост сохранности выбранных фрагментов и устранение конкретного P01 selector miss; рост direct recall на новом наборе **не подтверждён**, исходный v2 уже достиг8/8 в корректном clock scope. Новых FP/FN/unknown direct на этой выборке нет. Не проверены свежий URL/source holdout, устойчивость на множестве LLM повторов, полный retrieval/initial classifier, published Firecrawl MCP backend, live fetching/public JS, VPS и нагрузка.
