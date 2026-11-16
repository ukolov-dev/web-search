# Собственная дата новости до coverage — 3 ноября 2026

В opt-in `request-evidence-v14-news-anchors` исправлена потеря собственной даты основной статьи среди боковых `<article>`-анонсов. Два прежних пропуска проходят date admission, Qwen, coverage и audit и получают direct в сохранённом компонентном контроле. Отдельно закрыто загрязнение body и уточнены новостные place/anchor-подсказки. На полном NEW12 v12→v13 recall остался 75→75%, FP 0→0; дополнительная пара также не дала роста. После её диагностики отдельный NEW6 v12→финальная v14 дал **9/13→13/13 (69,23→100%)**, FP 0→0, unknown direct 0→0. Все попытки и наборы сохранены отдельно; компонентный успех не доказывает прирост всего поиска.

## Причина и исправление

В [предыдущей итерации](news-coverage-2026-11-03.md) два прочитанных Bezformata-источника не дошли до coverage: [поезд](https://nnovgorod.bezformata.com/listnews/poezd-svyazhet-kazan/165476847/) и [диспетчерский пункт](https://nnovgorod.bezformata.com/listnews/punkt-dlya-otdiha/165482058/). В исходном HTML есть собственное `NewsArticle.datePublished=2026-11-03`, связанное с URL и главным заголовком. Однако v12 требовала ровно один `<article>` во всём документе; фактически их 13 и 5 из-за других публикаций. Тип страницы распознавался как article, а microdata date extraction целиком пропускалась.

Новый общий `news::main_article_scope` выбирает единственную основную статью, содержащую единственный глобальный h1. Заголовок сам должен принадлежать этой статье. Явные типы ограничены Article/NewsArticle/BlogPosting; собственные itemid, url и ненестированная mainEntityOfPage должны совпасть с фактическим URL. Пустые, чужие и конфликтующие идентичности остаются отказом. Вложенная WebPage — отдельная сущность: её корневой itemid не переопределяет URL основной Article.

Даты вложенных Article/Event, aside/nav/footer, других teaser-ссылок, modified/event/copyright и неполные даты не становятся публикационной датой статьи. Неявная статья не наследует чужой внешний itemscope. Неоднозначные h1/overlapping article, invalid/conflicting dates, будущие timestamps и несоответствие запросному окну остаются недоказанными. Часы и год не угадываются.

Тот же selector используется для извлечения текста явно размеченной основной статьи перед generic `<main>`: `extraction_method=owned_article`, `content_scope=article`. Вложенные article и Article/NewsArticle/BlogPosting-сущности удаляются из её текста; собственное тело сохраняется. App/root guards, DOM/body limits и прежние fallback-пути сохранены.

Coverage и audit получают дополнительную подсказку **только при валидном доверенном новостном контексте**: событие должно относиться к запрошенному городу; имя издателя, contentLocation, область, соседний город и чужой teaser не доказывают эту связь. Конкретная остановка маршрута или однозначное городское учреждение в собственном теле могут её подтвердить без буквального именительного падежа. Запрос региона оценивается как запрос региона. В финальной C собственный фактический заголовок также может подтвердить non-core место того же события при содержательном S/B core; подробнее ниже. Неновостные coverage/audit prompts сохраняют прежнее содержимое; это проверено отдельно.

Правки: [news.rs](../src/news.rs), [extract](../src/main.rs), [news prompts](../src/model.rs), version tag/stdio и regressions. В [news_search.rs](../src/news_search.rs) добавлен только test-only named replay; production navigation не менялась. Новых зависимостей, моделей, поисковых/скрейпинговых API, retries, VPS и глобальных настроек нет. Default legacy сохранён; v14 — прежний startup `--evidence-memory` с `--reranker-settings`.

## Development: все попытки

Сохранённые два HTML — original-validation snapshots предыдущего сеанса, не byte-identical первичные model inputs. Их hashes, фактические URL/hops/status и исходные часы проверяются replay. Начальные labels выставлены только ambiguous; retrieval и initial classifier в этом компонентном повторе отсутствуют. Реальные Qwen, coverage, audit и финальный news gate выполняются обычными функциями продукта.

| Проверка до NEW12 | v12 / первая date-only v13 / финальная v13 |
| --- | --- |
| Одинаковые два сохранённых источника | v12: admitted0/direct0, генераций/Qwen0; date-only v13: admitted2/direct2, 2 генерации, 2 Qwen/6 pairs |
| Полный живой прежний запрос «новости Нижнего Новгорода» | v12: direct0; date-only v13: 3 direct, из них две подходящие статьи и один неподходящий региональный TV-сюжет |
| Сохранённые old2 + этот отрицательный сюжет, одинаковые источники/часы | v12: admitted0/direct0; финальная v13: admitted3, **2 direct + 1 background**, 2 генерации и 3 реальных Qwen, без нового поиска/HTTP |

Неподходящий [TV-сюжет](https://nnovgorod.bezformata.com/listnews/privolzhe-glavnie/165483239/) сообщает о Городце, областных дорогах и Москве. В первом date-only прогоне модель принимала цитату о Городце за условия Нижнего Новгорода; audit соглашался. Имя «Нижний Новгород» было в издателе/метадате и связанных материалах. Это **реальный development FP**, он не удалён и не переименован в положительный.

Generic `<main>` также включал соседние публикации. Изоляция основного тела исправляет это отдельное загрязнение, но первоначальная ошибочная цитата уже относилась к собственному сюжету о Городце. Поэтому устранение FP относится к сочетанию собственного scope и city-guidance/новой модельной проверки; нельзя приписывать весь эффект одному extraction-патчу. Контрольный gold заморожен до финального model replay, источник отдельно оценён участником без просмотра predictions.

Детерминированные regression tests показали red→green для отсутствующей даты и попадания sibling/nested teaser в body. Peer до нового набора нашёл и помог закрыть три ошибки ownership: чужую direct mainEntityOfPage, наследование Event и заголовок из sidebar/чужой сущности. Они сохранены как adversarial controls, не маскируются первыми зелёными тестами. [Scoped bug report](../outputs/news-dates-2026-11-03/bug-reproducer-report.md), [old2 replay](../runs/news-dates-2026-11-03/old2-replay-summary.json), [финальный positive/negative replay](../runs/news-dates-2026-11-03/old-city-control-summary.json), [peer review](../runs/news-dates-2026-11-03/safety/body-city-safety-review.md).

## NEW12 v12→v13: процедура и разметка

[NEW12 cases](../feasibility/news-dates-cases-v1.json) и [plan](../feasibility/news-dates-plan-v1.json) заданы до новых вызовов: девять текущих/вчерашних возможностей в нескольких городах, архивный месяц, будущая дата и wrong-site контроль. nd08 — заранее выделенная source-aware transport-проба. Все 12 точных формулировок отсутствуют среди 217 нормализованных прежних запросов; города/издатели/URL могут повторяться. В частности, nd01/nd08 могут пересекаться с development. Это query-heldout, не URL-heldout и не независимая общая квалификация.

Baseline — сохранённый v12 release; candidate — финальный v13. Модель закреплена: OpenRouter `deepseek/deepseek-v4.1-flash-1017`, локальный `Qwen3-Reranker-0.6B-4bit`. По очереди запускаются две ветки с чередованием порядка. Каждый primary call использует существующий CLI `ranked-search`: тот же `policy::Session::run` и `public_result(...,10)`, что MCP, с новым Session/process. MCP проверен отдельно release-stdio; primary **не является persistent-MCP benchmark**.

До inference первоначальный persistent-MCP draft заменён явно сохранённой transport-amendment: CLI `--output` архивирует HTML, полный extract и model inputs/outputs при самом запросе. Это даёт точные первичные оригиналы без дополнительных чтений источников. После найденного development FP scope плана дополнен body isolation и city-guidance; запросы, критерии и метрики не менялись. Все ранние plan/protocol/tooling snapshots сохранены.

Ветви live: одинаковые bytes/model inputs между ними не гарантированы. Все наблюдаемые варианты первичного HTML сохраняются; при разметке важны собственное тело/дата и отсутствие неразрешённого конфликта вариантов. Blind inputs скрывают arm, direct/background, runtime type/date verdict и признак read/returned. В полном baseline extract могли быть чужие teaser: это не делает их собственной статьёй при source-only grading.

Primary denominator заранее определён как общее множество известных подходящих case/URL из **успешно прочитанных ИЛИ возвращённых** страниц двух выполненных веток. Прочитанный, но потерянный до top10 положительный источник остаётся FN. Returned-only recall — вторичная сопоставимая метрика. Unknown и неполученные/неполные оригиналы учитываются явно, не превращаются в FP или отрицательные страницы. Нулевой denominator — `null`.

Grade2 требует полноценную собственную статью по городу/теме/site/публикационному окну с полной собственной датой. Modified/Event, copyright, URL-year, query и соседний teaser не заменяют публикацию. Ленты/навигация — grade1, явное несоответствие — grade0, существенная неопределённость — unknown. Это пригодность источника к запросу, не проверка истинности каждого его факта.

## Результаты NEW12

**На основном NEW12 прироста нет: primary direct recall 3/4→3/4 (75→75%), FP0→0, unknown direct0→0.** Все 24 CLI calls выполнены, но 4 из них пустые из-за двух запросов с parser_changed. Safety gate прошёл в своей ограниченной постановке; usefulness gate на положительный recall delta не пройден.

До соединения с predictions заморожены 114 source-only case/URL:30 grade0,73 grade1,4 grade2,7 unknown. В разметке использованы32 уникальных первичных URL / 52 наблюдавшихся варианта,0 пропущенных оригиналов среди успешныхREAD; один extract усечён и проверен по raw HTML. Эти числа не включают неуспешные чтения как полученные оригиналы. Вызовов additional validation 0.

| Запрос | Known-positive READ+returned union | TP v12→v13 | FP v12→v13 |
| --- | ---: | ---: | ---: |
| nd01: Нижний Новгород,03.11 | 0 | 0→0 | 0→0 |
| nd02: Казань,03.11 | 0 | 0→0 | 0→0 |
| nd03: Екатеринбург,сегодня | 1 | 0→0 | 0→0 |
| nd04: Тула,сегодня | 0 | 0→0 | 0→0 |
| nd05: Пермь,сегодня | 2 | 2→2 | 0→0 |
| nd06: Уфа,сегодня | 1 | 1→1 | 0→0 |
| nd07: Нижний Новгород,02.11 | 0 | 0→0 | 0→0 |
| nd08: site Bezformata,транспорт,03.11 | 0 | 0→0 | 0→0 |
| nd09: Нижний Новгород,ноябрь2025 | 0 | 0→0 | 0→0 |
| nd10: Казань,03.11.2030 | 0 | 0→0 | 0→0 |
| nd11: site Химки/новости Воронежа | 0 | 0→0 | 0→0 |
| nd12: Екатеринбург,02.11 | 0 | 0→0 | 0→0 |
| **Всего** | **4** | **3→3** | **0→0** |

В каждой ветке 100 returned rows. Вторичный returned-only recall3/3→3/3 (100→100%) был бы слишком оптимистичен без primary denominator: четвёртая подходящая статья прочитана, но не возвращена. Все 3 TP одинаковы: две публикации 59.ru в Перми и статья UFA1 о Колхозном рынке. Всего 4 положительных события / 3 доменные группы источников; TP — 3 события / 2 домена. Это не независимые повторения качества по всему вебу.

Единственный known FN у обеих веток — [публикация программы праздника в Екатеринбурге](https://xn--80acgfbsl1azdqr.xn--p1ai/news/102521-den-narodnogo-edinstva-kak-prazdnik-proydet-v-ekaterinburge). Собственная дата «3 ноября 2026, 14:31» находится в `.news-one-date` внутри обычного `div.news-one`; Article-разметки нет. Обе версии прочитали полный текст, но metadata.kind=feed/date_status=missing, gate reason=not_article: coverage/Qwen не запускались для этой страницы. Дата 4 ноября в статье — день события, а не публикации. Этот формат не добавлялся по результату NEW12.

nd08 и nd11: по0 результатов у обеих версий, `seznam: parser_changed` и `naver: parser_changed`; sensitivity/wrong-site probes не проверили заявленную семантику на найденных статьях. nd10: три UTF-8 errors в каждой ветке; nd12: один HTTP404 в каждой. Ошибки не заменены удачными запросами, повторных загрузок нет. Все 20 фактически вызванных initial classifiers валидны; 4 empty calls имеют classifier_valid=true только потому, что candidates0, и не считаются 20+4 успешными модельными классификациями.

На датированных длинных запросах NEW12 исправленная microdata-ветка не дала дополнительных подходящих статей: поисковая выдача и отбор источников остались ограничением. После пустой site-пробы выполнены ровно два коротких supplementary queries без изменения v13/основного score. Их адаптивный, source-aware результат опубликован отдельно.

Данные: [результаты](../feasibility/news-dates-results-2026-11-03.json), [gold](../feasibility/news-dates-gold-2026-11-03.json), [independent recount](../runs/news-dates-2026-11-03/benchmark/independent-metrics-review.json), [protocol audit](../runs/news-dates-2026-11-03/benchmark/independent-protocol-review.json). Gold SHA256 `19bf23f844fd25386562f99ec8d5811dbbc973d836431d0d9ca7e83f9f12ffb9`; оба source-only halves frozen доjoin. Отдельный source-only cross-review подтвердил три положительных Perm/Ufa статьи, без доступа к predictions и без изменения gold.

### Отдельная supplementary sensitivity-проба

[Дополнительная пара](../feasibility/news-dates-sensitivity-cases-v1.json): «последние новости Нижнего Новгорода» и «последние новости Челябинска», четыре вызова v12/v13. Primary READ+returned: **0/2→0/2 (0→0%), FP 0→0, unknown direct 0→0**. В первом запросе положительных источников нет, recall `null`. Во втором две собственные статьи 74.ru — дорожное изменение и репортаж о стоковом магазине — прочитаны, но обе остаются FN. Это два события одного домена, а не независимые повторения по издателям.

До join заморожены 20 source-only case/URL: 7 grade0, 10 grade1, 2 grade2, 1 unknown; 7 уникальных URL / 11 вариантов первичных оригиналов, без дополнительного HTTP. На ветку: 4 logical searches, 3 генерации, 8 чтений, 3 Qwen; 21→22 pairs. У первого запроса UTF-8 error C006 в обеих ветках; все четыре вызова сохранены, повторов нет.

Причина двух FN — реальный ответ coverage: anchors `[последние, новости, Челябинска]`, core и первые два условия supported, активный R3 города missing с пустой цитатой. Поэтому audit не вызывался. Это **не** title-copy/witness rejection; валидатор правильно сохранил missing. [Результаты](../feasibility/news-dates-sensitivity-results-2026-11-03.json), [gold](../feasibility/news-dates-sensitivity-gold-2026-11-03.json), [отдельный recount](../runs/news-dates-2026-11-03/sensitivity/independent-metrics-review.json).

### V14: от формулировки anchors к проверке места

После supplementary development prompt уточнён только для trusted news context. «Новости» и слова свежести не должны тратить anchor slots, если они лишь задают тип запроса/доверенное окно; фактические город, тема, версия и полный исходный запрос остаются обязательными. Checks должны соответствовать порядку выбранных anchors. Парсер не чинит и не удаляет активный missing.

Первые варианты A и B не помогли сохранённым челябинским статьям: модель выбрала один городской anchor, но оставила его missing (0/2 direct). Во всех сохранённых нижегородских контролях обе подходящие статьи проходили, региональный TV-сюжет оставался background. Эти неудачные варианты и ответы сохранены; финальная C отличается текстом инструкции, это не повтор одинакового prompt ради удачного ответа.

В финальной C явно разделены core и non-core place: собственный фактический главный заголовок может связать место **того же события** с содержательной body-цитатой. Заголовок издателя, обращение к аудитории, suffix сайта и чужой teaser связи не доказывают. Ограничение core на содержательный S/B, запрет простого повтора заголовка, exact witnesses, активные условия и skeptical audit сохранены. На одинаковых сохранённых челябинских источниках C дала две статьи direct и quiz background; нижегородский контроль — две статьи direct и TV background. На каждый контроль 2 генерации / 3 реальных Qwen, retrieval/initial classifier/нового target HTTP нет.

Regression `saved_news_city_check_remains_mandatory_despite_generic_anchors` на фактическом model reply проверяет: активный missing города блокирует direct даже при принудительно положительном audit. Это сохранение границы, **не red→green**. Неновостные prompts побайтно прежние. [Все A/B/C controls](../runs/news-dates-2026-11-03/v14-controls-summary.json), [финальный peer review](../runs/news-dates-2026-11-03/safety/v14-core-place-clarification-peer-review.json).

## Отдельный NEW6: финальная v14

[NEW6 cases](../feasibility/news-dates-confirm-cases-v1.json) и [протокол](../feasibility/news-dates-confirm-plan-v1.json) заморожены до inference финальной C. Пять запросов текущих новостей: «свежие новости Челябинска», «последние новости Перми», «свежие новости Уфы», «последние городские новости Екатеринбурга», «свежие городские новости Омска»; шестой — «новости Челябинска транспорт 3 ноября 2030». Текущие окна 1–3 ноября 2026, будущее окно 3 ноября 2030. Новизна — точные строки относительно 231 прежней нормализованной формулировки в feasibility, а не города, URL или издатели. Audit подтверждает наличие этих строк, не выполнение каждой.

Все 12 вызовов выполнены в замороженном порядке с чередованием веток, без ошибок источников, повторов или подмены запросов. В каждой ветке 12 logical searches, 24 HTTP reads, 14 Qwen / 87 pairs; generative v12→v14: 14→16. Максимальный context 17997→17163 символа, ниже 24000. Архивирование первичных HTML, extract и model calls не добавляло HTTP. Source/binary/tooling hashes совпали после завершения. Все 12 initial classifiers реально вызывались и валидны; vacuous empty-классификаций нет.

Source-only bundle: 24 уникальных первичных URL / 42 варианта HTML, 64 case/URL cards, без пропущенных оригиналов среди успешных READ. Разметка идёт по каждому запросу отдельно; одна статья в разных окнах может иметь разную пригодность.

Это адаптивный, mechanism-sensitive/source-aware confirmation после development; объединять его score с NEW12 и supplementary нельзя. Первые пять формулировок намеренно проверяют generic recency/news descriptors, вызывавшие дефект. Первоначальный набор не запускался; пять формулировок уточнены до inference, future ca06 сохранён. Исходный набор и правки сохранены как pre-inference amendment; критерии и метрики не менялись. **Primary READ+returned direct recall: 9/13→13/13 (69,23→100%, +30,77 п.п.). FP 0→0, новых FP IDs — 0, unknown direct 0→0.** Все 13 подходящих источников теперь direct, известных FN в этом наборе — 4→0. Scoped safety и положительный recall delta прошли; это не общая квалификация.

| Запрос NEW6 | Known-positive union | TP v12→v14 | FP v12→v14 |
| --- | ---: | ---: | ---: |
| ca01 Челябинск | 3 | 3→3 | 0→0 |
| ca02 Пермь | 2 | 0→2 | 0→0 |
| ca03 Уфа | 2 | 0→2 | 0→0 |
| ca04 Екатеринбург | 3 | 3→3 | 0→0 |
| ca05 Омск, новый город | 3 | 3→3 | 0→0 |
| ca06 Челябинск, публикация 03.11.2030 | 0 | 0→0 | 0→0 |
| **Всего** | **13** | **9→13** | **0→0** |

Четыре baseline FN — обе статьи Перми и обе Уфы — уже прошли собственную дату, article admission и Qwen. Coverage выбрала `[последние/свежие, новости, город]`, первые условия supported, но активный R3 города оставался missing с пустой принятой цитатой; audit не запускался. Headline-core guard и потеря фрагментов не были причиной. У одной пермской статьи `ignored_non_support_quote` очистил цитату при уже missing статусе, не превратил поддержанное условие в missing. В v14 выбран один содержательный городской anchor, core/place checks поддержаны, затем выполнен положительный audit. Город не удалён из требований и missing не принудительно повышен. [Точные ответы и четыре перехода FN→TP](../runs/news-dates-2026-11-03/confirm/four-baseline-fn-diagnostics.json). Live bytes/ranges между ветками различались, поэтому это наблюдаемый механизм совокупного патча, а не изолированный контрфактуальный эффект одной строки prompt.

В каждой ветке 60 returned rows. Secondary returned-only denominator также 13, score совпадает с primary; скрытых known-positive READ вне returned в NEW6 нет. У будущего контроля recall `null` — подходящих публикаций 2030 года нет. На новом городе Омске 3/3→3/3, рост сосредоточен в ранее виденных городах/доменах Перми и Уфы. Всего 13 событий / 5 доменных групп: 74.ru, 59.ru, UFA1, E1, SuperOmsk. Это не 13 независимых экспериментов и не 5 независимых издательств.

Gold до join: 17 grade0, 27 grade1, 13 grade2, 7 unknown. Два source-only half заморожены отдельно; перекрёстная проверка всех 13 положительных страниц подтвердила их без разногласий и правок gold, по всем относящимся вариантам. В омской статье о рейсах собственные publication metadata/header согласованы на 3 ноября, но body содержит календарную аномалию заявления «во вторник, 4 ноября». Она сохранена как ограничение истинности/хронологии источника; event date не присвоена публикации. Grade2 оценивает пригодность собственной публикации, а не внешнюю достоверность каждого утверждения.

[Результаты NEW6](../feasibility/news-dates-confirm-results-2026-11-03.json), [gold](../feasibility/news-dates-confirm-gold-2026-11-03.json), [отдельный stdlib-пересчёт](../runs/news-dates-2026-11-03/confirm/independent-metrics-review.json), [protocol audit](../runs/news-dates-2026-11-03/confirm/independent-protocol-review.json). Gold SHA256 `8224af96a398ba761c3649dd49ed8bb85bab34c66b9c911f6d1059482e6c5a95`. Пересчёт написан отдельно от scoring helpers и подтвердил exact TP/FN IDs, FP/unknown counts; это проверка расчёта внутри команды, не formal acceptance.

## Проверки и ограничения

Финальный код: **115 unit + 5 stdio = 120 ordinary tests passed**, fmt, clippy `--all-targets -D warnings`, offline release build и пять release-stdio checks прошли. Общий `--ignored` не запускался; только конкретный сохранённый stage replay. Все API/model/URL/DNS/IP/redirect/site guards сохранены; source-block прекращает обе оставшиеся ветки без retries/reset. andro.io/subdomains не являются разрешёнными validation targets.

На запрос остаются 2 logical searches, 3 генерации, 4 HTTP-чтения, 4 Qwen, 180 с и 24000 source chars на модельный вызов. Статья из ленты входит в общий budget; архивирование primary не добавляет HTTP. NEW12 на каждую ветку: 24 logical search, 14 generative, 40 HTTP reads; Qwen v12:4 calls/22 pairs, v13:3/12. Это наблюдаемые суммы, не изолированный speed/quality тест reranker. Дополнительного HTTP для primary source gold — 0. Сохранённые stage controls, полный development pair и A/B/C prompt controls считаются отдельно от новых наборов; все реальные model calls и raw usage сохранены. Денежные расходы не являются критерием этого сравнения.

Улучшение допуска источников снимает прежний случайный барьер: это может открыть существующую переоценку релевантности моделью, как и произошло в development. Новая подсказка не доказывает общий zero-FP. Внутри каждого набора runtime, queries, рубрика и tooling не донастраивались; переход v13→v14 сделан между завершёнными наборами с новым протоколом; исходные 20/B17 и исторический gold не меняются. Сохранены baseline/final/неудачные initial source/binary snapshots; Git не инициализировался.


Предыдущий OLD2 компонентный успех и новый search recall отвечают на разные вопросы. Допуск дат исправлен причинным red→green; semantic prompt проверен эмпирически, причём первые варианты не помогли. Финальный source-aware confirmation проверяет совокупность изменений v12→v14, а не изолирует вклад даты, Qwen или anchors. Новые ошибки нельзя устранить подменой gold/запроса или повтором текущего набора.

Не выполнены day2, Linux/VPS, нагрузка и общая независимая приёмка пилота. Формат `.news-one-date` у неразмеченного городского портала остаётся known gap; его адаптер в эту итерацию не добавлялся. Project Context сохраняет фактическую verification и partial finalization. [Project Context workflow](../.agents/skills/project-context-workflow/SKILL.md) требует: «`completion: complete` requires a fresh independently accepted `ready_to_merge` execution run». Успешные тесты и peer review команды его не заменяют. Это ограничение статуса приёмки, а не ожидаемое согласование уже запрошенной реализации.


Память разработки обновлена: TASK-20261103-153636-001, verification VERIFY-20261103-173605-001, research revision24. `finalize-work` вернул FINALIZED / completion=partial / requiresHumanReview=false. Lint, index check и context smoke прошли. Doctor завершился WARN: Git ignore/tracking не проверяемы без Git, source-coverage видит 12504 кандидата вне glob, ready backlog отсутствует; ссылки/схемы/индекс/модули исправны. Это не чистая независимая приёмка. [Сводка выполнения](../feasibility/news-dates-execution-2026-11-03.json), [разбор пропусков](../feasibility/news-dates-misses-2026-11-03.json), [проверка сохранности](../runs/news-dates-2026-11-03/final-integrity.json).
