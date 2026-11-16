# Потери extraction и отбора документов: исправление и полный A/B

30.10.2026, задача `TASK-20261030-175322-001`. Продолжение [проверки v4](reliability-full-search-2026-10-30.md). Собственные инженерные проверки и отдельная слепая разметка источников; независимой приёмки пилота нет.

**Релевантные источники стали чаще доходить до чтения и returned top10: 11/13 → 12/13, recall84,6% → 92,3%. Direct recall остался61,5% (8/13). Ложных direct стало4 → 2; новых ложных ID нет.** Улучшение отбора подтверждено, рост конечного direct recall этим прогоном не доказан. Два оставшихся false direct не скрыты.

## Что изменилось

### Извлечение явно размеченного тела документа

Сохранённый HTML `git-clean` содержит SYNOPSIS, OPTIONS, CONFIGURATION, но Readability выбирал лишь Interactive mode. Контейнер автора `<div id="main" data-pagefind-body>` не распознавался существующим semantic selector.

Добавлен один общий selector **уникального `[data-pagefind-body]`** перед fallback Readability. Приоритет существующих semantic main/article, удаление navigation/script/footer, однократная сериализация, HTTP/size/time limits и hashes сохранены. Это обработка структуры документа, без условия на домен Git. Если контейнеров несколько или явной разметки нет, остаётся прежний fallback. `coverage=not_verified` сохранён: наличие текста не доказывает полноту всех страниц.

На настоящем retained fixture текст увеличился **1910 → 4594 символа**, восстановлены OPTIONS, `--dry-run`, `-x`, `-X`, CONFIGURATION, а navigation/changelog/footer в результат не попали. Тот же regression test: exit101→0; все6 extraction tests прошли.

Дополнительно оба настоящих CLI-extractor offline обработали одинаковые34 HTML свежего набора:29 результатов совпали,4 ошибки не изменились,1 документ восстановлен — **git-stash4508→16147 символов, readability→declared_body**. Три заранее замороженных gold witnesses о конфликте, сохранении stash и различии apply отсутствовали в старом извлечении и присутствуют в новом. Это измерение покрытия конкретных свидетельств, а не вывод о качестве по длине текста. [Сравнение по источникам](../feasibility/extraction-selection-extraction-2026-10-30.json), [скрипт](../scripts/compare-extraction-selection-sources-2026-10-30.py).

### Один шанс чтения для reference с бедным сниппетом

V4 выбирала до10 direct/background и читала первые4. Initial irrelevant исключал источник окончательно, даже когда карточка содержала только boilerplate перевода. На сохранённом CSS-примере так терялась релевантная MDN-страница.

В evidence memory **v5 `request-evidence-v5-reference-read`** резервируется максимум одно из четырёх чтений для reference-URL, пересекающегося со словами запроса. Признаки структуры адреса: host `docs.` либо сегменты `docs/doc/reference/manual/api`; выбор по числу общих слов, при равенстве — прежний порядок. Это грубая эвристика пригодности для чтения, без whitelist доверенных сайтов. Прежние первые три позиции сохраняются; выбранный reference ставится не дальше четвёртой. Источник с initial irrelevant может получить этот шанс, но не автоматическую метку background/direct.

Сохраняются site-фильтр, URL/DNS/IP/redirect guards и общий предел **10 кандидатов / 4 попытки чтения / 3 генерации / 180 секунд**, до4 local reranks внутри deadline. Initial classifier, coverage, literal guards, точные witnesses и обязательный skeptical audit **не ослаблены**. URL не служит доказательством. Без accepted audit источник не становится direct.

Regression на настоящих старых source cards/initial labels теперь включает потерянный reference в4-read pool; вариант с несвязанным reference URL исключается, и ни один непроверенный кандидат не повышается. Тот же targeted command: exit101→0. Default legacy сохранён; новый отбор включается через прежний startup `--evidence-memory`.

## Протокол нового эксперимента

Заранее зафиксированы8 новых вопросов,149 карточек, финальные исходники и hashes. Baseline — **текущая v4**, уже с исправленным initial; fixed — v5 с новым extraction/selection. В обеих ветках `deepseek/deepseek-v4.1-flash-1017`, initial reasoning none/max4096 и **локальный Qwen3-Reranker-0.6B-4bit через oMLX0.7.0**. Ни initial, ни audit не обходились. Initial messages/schema/token/reasoning profile одинаковы; ответы генерировались заново, поэтому модельная вариативность остаётся фактором.

Seznam/Naver реально опрошены один раз:16 ответов200. Обе ветки получают одинаковую замороженную выдачу. В отличие от прошлого harness, теперь общим кэшируется **сырой результат acquisition: HTML/status/final URL/hops/hash либо ошибка получения**, а не уже извлечённый текст. Каждая ветка запускает собственный extractor и обычные navigation/site/status guards. Extraction failure не превращает сохранённый HTML в acquisition failure. Отдельный offline test проверяет это, hash corruption, timeout, disabled host и non200. Test-only seam отсутствует в release.

По веткам получено36 уникальных raw acquisition outcomes:34 HTML и2 timeout. Старые host blocks унаследованы, повторы после блокировки и обходы не выполнялись. Один заранее выбранный MDN overflow URL прочитан отдельно только для gold и не передавался моделям. Одна первая сборка baseline не нашла compile-time fixtures: лог сохранён, fixtures добавлены, до сети/генераций она не дошла. У baseline уникальное имя Cargo package; версии и binary hashes проверяются до генераций.

Gold размечен отдельным агентом по source cards и полному сохранённому HTML, **без чтения model outputs/labels**. Заморожен после inference, до раскрытия предсказаний. Это один автор, частичное покрытие, не независимая приёмка. Из149 карточек63known:13positive и50partial/unrelated;86unknown. Дословные source witnesses с hashes/UTF-8 ranges проверены. Непросмотренные/недоступные источники не объявлялись нерелевантными. Gold-only документы в рабочий pipeline не добавлялись.

До раскрытия labels обнаружено, что n01C020 утверждает удаление stash при конфликтном pop, вопреки явно процитированному правилу официального Git. В основном gold такой источник grade1 из-за неверного материального условия; обе цитаты и `source_conflict` сохранены. Альтернативный source-relevance-only подсчёт указан ниже. Это решение не принималось по результатам моделей.

## Результаты полного поиска с reranker

| Метрика | Baseline v4 + Qwen | Fixed v5 + Qwen |
| --- | ---: | ---: |
| Корректный initial | 8/8 | 8/8 |
| Initial length / stop-empty / reasoning tokens | 0 / 0 / 0 | 0 / 0 / 0 |
| Известные positive в coverage pool | 11/13 | **12/13** |
| Известные positive выбраны/прочитаны/извлечены | 11/13 | **12/13** |
| Read recall | 84,6% | **92,3%** |
| Релевантные returned top10, любой класс | 11/13 | **12/13** |
| True direct / false direct / FN | 8 / 4 / 5 | **8 / 2 / 5** |
| Unjudged direct | 0 | 0 |
| Direct recall | 61,5% | **61,5%** |
| Precision среди known direct | 66,7% | **80,0%** |
| Генерации / target read attempts | 23 / 32 | 23 / 32 |
| Local rerank calls / pairs | 26 / 550 | 27 / 600 |
| Reranker fallback | 0 | 0 |
| Проверенные source proof ranges | 59 | 50 |
| Полный replay median latency | 18,498с | 16,042с |
| Reported provider cost | $0,003875394 | $0,003736308 |

Все46 генераций обслужил Morph. Общая известная reported cost **$0,007611702**, плюс3 release MCP smoke генерации без выгруженного usage: их цена неизвестна, не нулевая. Local scoring в provider cost не входит. Последовательный replay использует разные cache hits (0→26), поэтому timing не является изолированным latency benchmark/SLA. Direct recall delta=0; descriptive query-bootstrap95% −20,0…+27,3п.п. Не доказано ни улучшение direct recall, ни эквивалентность на большом корпусе.

| Вопрос | Positive | TP v4→v5 | FP v4→v5 |
| --- | ---: | ---: | ---: |
| Git stash pop/apply: сохранение при конфликте | 1 | **0→1** | **3→1** |
| Python groupby: adjacent/shared iterator | 1 | 1→1 | 0→0 |
| Rust BufRead: UTF-8/error/delimiter | 1 | 0→0 | 0→0 |
| PostgreSQL DISTINCT ON / ORDER BY | 2 | **2→1** | 0→0 |
| SQLite REPLACE: triggers/hook/counter | 1 | 0→0 | 0→0 |
| Docker empty volume / nocopy | 1 | 1→1 | 0→0 |
| CSS hidden/clip: programmatic scrolling | 2 | 0→0 | 1→1 |
| HTTP HEAD / GET / Content-Length | 4 | 4→4 | 0→0 |

Новый выбор доставил официальный Git stash до чтения и direct, а новый extractor сохранил его ключевые свидетельства. Из старых FP удалились n01C001/C015; **новых false-direct ID нет**. Но на PostgreSQL один прежний TP стал background: в новом ответе coverage изменились anchors/cited passages, и literal guard не нашёл ORDER в них. Документ прочитан в обеих ветках; это оставшаяся модельная/fragment-coverage потеря, а не отсутствие документа. Поэтому +1 полезный direct по Git компенсировался −1 по PostgreSQL. На общих прочитанных страницах extraction hashes совпали; full A/B не изолирует отдельный вклад extractor от selection и вариативности генераций. Дополнительное offline-сравнение raw HTML отдельно подтверждает восстановление трёх Git stash witnesses; исправление OPTIONS также доказано retained-source regression.

## Оставшиеся ошибки и вывод

- **n01C020** всё ещё direct в обеих ветках, хотя неверно описывает конфликтный pop. Gate проверяет наличие цитаты и релевантность, но не разрешает межисточниковое противоречие. Наличие official Git рядом не исправило эту ошибку автоматически.
- **n07C015** — старый design review `clip`, с нерешённым сравнением и пометкой unsatisfied. Обе ветки ошибочно приняли его как direct: свидетельства/контекст не донесли статус предложения достаточно надёжно.
- Rust/SQLite уже прочитаны, но их цитируемые фрагменты/coverage не закрыли все условия. Полезный MDN overflow остался вне чтения: один reference slot не гарантирует оптимальный выбор из нескольких родственных reference-страниц.
- Четыре extraction failure в baseline против трёх в fixed и два общих timeout наблюдаемы; часть HTML по-прежнему не извлекается. Не размеченные контейнеры и отсутствующие тексты Qwen восстановить не может.

Если считать ошибочный n01C020 только тематически релевантным (`source_relevance_grade=2`), в sensitivity получается TP9→9,FP3→1,FN5→5, recall64,3%→64,3%. Основной вывод тот же: отбор улучшился, конечный direct recall не вырос, оставшийся CSS FP сохраняется. Основной строгий подсчёт выше не выдаёт неверный материальный факт за полезное свидетельство.

Для дальнейшего direct recall нужен отдельный эксперимент с покрытием **всего набора условий** в selected fragments и устойчивостью anchors; для оставшихся FP — сохранение статуса proposal и проверка явных противоречий между источниками. Это предложения следующего этапа, не скрытая донастройка по нынешнему набору. Этот8 после публикации становится regression/development.

## Проверки и артефакты

Прошли59 обычных тестов (54unit+5stdio), fmt, all-targets clippy `-D warnings`, release build и5 stdio тестов свежего release. Реальный release MCP smoke без raw cache: **status=ok, errors0, initial valid,3 генерации/4 чтения/4 Qwen calls/61pairs**. Это проверка опубликованного пути, не девятый quality-кейс. Source-specific own review не обнаружил actionable issues; Router refactor heuristic без Git не получил настоящий diff и не считается semantic acceptance.

- [Новые8 запросов](../feasibility/extraction-selection-cases-v1.json), [слепой gold](../feasibility/extraction-selection-gold-v1.json).
- [Durable corpus](../feasibility/extraction-selection-corpus-2026-10-30.json): raw HTML, branch-specific extracted documents, model inputs/outputs, manifests, gold и freeze hashes; [метрики/per-case](../feasibility/extraction-selection-results-2026-10-30.json).
- [Учёт acquisition/стоимости/FP](../feasibility/extraction-selection-execution-2026-10-30.json), [native red→green report](../outputs/extraction-selection-2026-10-30/bug-reproducer-report.md).
- [Gold builder](../scripts/extraction-selection-gold-2026-10-30.py) запрещает перезапись frozen gold; [анализатор](../scripts/analyze-extraction-selection-2026-10-30.py) запускается offline командой `python3 scripts/analyze-extraction-selection-2026-10-30.py`.

Context lint/index freshness/smoke прошли; doctor15ok/5warn/0fail (предупреждения Git/tracking, широкого tooling coverage и ready backlog). Verification evidence и last_result сохранены в Router; формальное completion partial, поскольку независимой ready_to_merge приёмки нет.

Логи находятся в `runs/extraction-selection-2026-10-30`; это временная evidence, не долговечная память. Original20 SHA-256 `b67e3269830ae0e976aa9b4c5dc2f6b9c3e922ad1adda35738289d6b1b8325d6`, B17 и URL safeguards сохранены. VPS, нагрузка, повтор в другой день, MiMo+v5 quality и независимая приёмка не выполнены. Default legacy не менялся; production body extraction общий для extract/ranked_search, reference selector только opt-in evidence memory.
