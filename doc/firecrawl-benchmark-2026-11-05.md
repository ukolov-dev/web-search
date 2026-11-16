# Firecrawl и текущий Rust reader: 12 страниц, 5 ноября 2026

**Замена текущего reader по этой проверке не обоснована.** Локальный Firecrawl извлёк одну дополнительную фоновую страницу и сохранил 17/17 контрольных фрагментов против 16/17. Direct recall остался 6/7 (85,71%), ложных direct — 0 у обоих. При этом одна пригодная статья потерялась, другая восстановилась. Медиана локальной обработки страницы выросла с 2,73 до 32,40 мс.

Это парный **компонентный эксперимент на одинаковых сохранённых HTML**, проведённый через Rust-клиент и существующие Qwen/coverage/audit. Полученные числа не измеряют свежесть новостей, retrieval recall, JavaScript-rendering или опубликованную MCP-интеграцию Firecrawl.

## Результат

| Метрика | Текущий Rust | Firecrawl → Rust |
| --- | ---: | ---: |
| Успешно извлечённые страницы | 11/12 | 12/12 |
| Сохранённые контрольные body-фрагменты | 16/17 | 17/17 |
| Страницы со всеми своими контрольными фрагментами | 11/12 | 12/12 |
| Собственная дата статьи и её тип на новостных страницах | 6/6 | 6/6 |
| Верные final direct / известные пригодные источники | 6/7 | 6/7 |
| Пропуски пригодных источников | 1 | 1 |
| Ложные direct на 5 отрицательных/фоновых страницах | 0 | 0 |
| Direct с неизвестным gold | 0 | 0 |
| Новые FP IDs | — | [] |
| Reader p50 / p95, мс | 2,729 / 10,960 | 32,403 / 153,610 |
| Фактические generative-вызовы | 8 | 8 |
| Qwen-вызовы / оценённые пары query–passage | 11 / 82 | 12 / 107 |
| Qwen fallback / ошибки группы | 0 / 0 | 0 / 0 |

Direct recall = TP / 7 заранее известных grade-2 страниц. Grade 1 означает полезный фон, которого недостаточно для запроса; grade 0 — неподходящий материал. Положительные страницы уже имели все 16 требуемых фрагментов у нашего reader. Дополнительный 17-й фрагмент Firecrawl относится к **P11, grade 1**: рост извлечения здесь не дал роста direct recall. Разметка и состав выборки заморожены до новых reader-результатов и model calls; это source-aware development, а не новый слепой holdout.

Сохранность означает точное включение заранее выбранной цитаты с нормализацией пробелов. Это проверка конкретных нужных фрагментов, а не полноты каждого символа статьи. Даты проверены нашим `news::metadata` по возвращённому `rawHtml`, хеш которого обязан совпасть с входным HTML. Поэтому 6/6 подтверждает сохранность исходных метаданных и работу нашего ownership/date/type gate; точность собственного поля Firecrawl `publishedTime` этим не измерена. У шести технических страниц дата N/A, даже если парсер нашёл историческую дату.

## Все страницы

В ячейках «A → B» слева текущий reader, справа Firecrawl. Дата каждой из первых шести страниц совпала в обеих ветках. Задержка — медиана трёх локальных reader-попыток на странице; ошибка P11 оставлена в результате.

| Страница | Gold | Body-фрагменты | Дата обеих веток | Final direct | Reader p50, мс |
| --- | ---: | --- | --- | --- | --- |
| P01 [Поезд Казань–Москва с остановкой в Нижнем Новгороде](https://nnovgorod.bezformata.com/listnews/poezd-svyazhet-kazan/165476847/) | 2 | 2/2 → 2/2 | 2026-11-03 | да → нет | 1.13 → 18.91 |
| P02 [Диспетчерский пункт на Ракетной](https://nnovgorod.bezformata.com/listnews/punkt-dlya-otdiha/165482058/) | 2 | 1/1 → 1/1 | 2026-11-03 | да → да | 0.73 → 12.58 |
| P03 [Региональный ТВ-дайджест](https://nnovgorod.bezformata.com/listnews/privolzhe-glavnie/165483239/) | 0 | 1/1 → 1/1 | 2026-11-03 | нет → нет | 1.52 → 25.91 |
| P04 [Тест по географии](https://74.ru/text/entertainment/2026/11/02/76696137/) | 0 | 1/1 → 1/1 | 2026-11-02 | нет → нет | 4.16 → 32.83 |
| P05 [Спорные дорожные знаки](https://74.ru/text/auto/2026/11/03/76741445/) | 2 | 2/2 → 2/2 | 2026-11-03 | да → да | 4.42 → 34.96 |
| P06 [Магазин забытых товаров](https://74.ru/text/business/2026/11/02/76729550/) | 2 | 1/1 → 1/1 | 2026-11-02 | нет → да | 7.13 → 62.76 |
| P07 [Neon: DISTINCT ON](https://neon.com/postgresql/tutorial/distinct-on) | 2 | 2/2 → 2/2 | N/A | да → да | 2.73 → 28.40 |
| P08 [Vonng: DISTINCT ON](https://vonng.com/en/pg/sql-distinct-on/) | 2 | 1/1 → 1/1 | N/A | да → да | 2.50 → 30.33 |
| P09 [Wikipedia: set operations](https://en.wikipedia.org/wiki/Set_operations_(SQL)) | 1 | 1/1 → 1/1 | N/A | нет → нет | 3.04 → 35.67 |
| P10 [Python: itertools](https://docs.python.org/3/library/itertools.html) | 2 | 3/3 → 3/3 | N/A | да → да | 10.96 → 149.11 |
| P11 [GeeksforGeeks: groupby](https://www.geeksforgeeks.org/python/itertools-groupby-in-python/) | 1 | 0/1 → 1/1 | N/A | нет → нет | ошибка → 24.39 |
| P12 [TutorialsPoint: group concatenate](https://www.tutorialspoint.com/article/python-group-concatenate-till-k) | 1 | 1/1 → 1/1 | N/A | нет → нет | 0.94 → 14.19 |

Запросы групп: F01 «новости Нижнего Новгорода», F02 «последние новости Челябинска», F03 `PostgreSQL DISTINCT ON ORDER BY first row unpredictable`, F04 `Python itertools.groupby adjacent equal keys shared underlying iterator`. Новостное окно воспроизводится по историческому clock 1–3 ноября 2026 года. Эти страницы не выдаются за актуальные новости на 5 ноября.

Точные 17 цитат, URL, исходные capture dates, прежняя разметка и SHA-256 находятся в `cases.json` / `gold.json` архива; постраничная сохранность и окончательные TP/FN/FP — в [машинном результате](../feasibility/firecrawl-results-2026-11-05.json).

## Почему direct не вырос

**P01: новый пропуск у Firecrawl.** Свидетельство города «Нижний Новгород-Стригино» осталось в полном извлечённом тексте. Но оно отсутствовало в контексте, переданном coverage после Qwen. Размер текста вырос с 1 425 до 7 138 Unicode-символов; выбранные Qwen UTF-8 ranges: `[7100,9291]`, `[3578,5661]`, `[1812,3905]`, тогда как начало нужной строки — byte 490. Core был supported, city — missing, audit не подтверждал страницу. Более широкий очищенный текст создал конкуренцию за ограниченные окна фрагментов; автоматического повышения в direct не было.

**P06: пропуск у текущей ветки, восстановление у Firecrawl.** Контрольная фраза «В магазинах с невыкупленными товарами» есть и в полном baseline-тексте, и в поданном Qwen-контексте. Baseline coverage оставил core missing при supported city; Firecrawl coverage и audit приняли оба условия. Объёмы близки: 12 684 и 12 818 символов. Один парный model pass не отделяет вариативность судьи от влияния изменённых passages; детерминированный причинный эффект извлекателя здесь не доказан.

В обоих пропусках `ignored_non_support_quote` относится к удалению цитаты у **уже missing** проверки. Это не отказ валидатора от supported-свидетельства. Точные статусы и offsets сохранены в `miss-diagnostics.json`. Нулевой FP не отменяет новой FN-регрессии P01.

## Как проверяли нашим клиентом

1. Из прежних реальных HTML-снимков выбраны 12 уникальных страниц: 7 пригодных, 5 фоновых/отрицательных, 6 с применимой новостной датой. Зафиксированы запросы, gold, witnesses и source hashes. Исходные original20/B17 и старые результаты не изменялись.
2. Baseline вызывает существующий Rust `extract`, используемый CLI/MCP. Candidate посылает **те же UTF-8 HTML bytes** в настоящий upstream `POST /v2/parse`, запрашивает `html`, `markdown`, `rawHtml`, `onlyMainContent=true`, `parsers=[]`. Возвращённый rawHtml проверяется SHA-256; cleaned HTML переводится в plain text тем же `dom_query::formatted_text`, который уже использует наш клиент. Подмены отсутствующего baseline-документа данными другой ветки нет.
3. Каждая страница обработана по три раза с чередованием веток и concurrency 1. Ошибка нашего P11 `failed to grab the article` повторилась во всех трёх попытках; Firecrawl вернул документ во всех трёх.
4. Четыре группы по три кандидата прошли текущий v14 evidence pipeline: `page_from_document` → локальный Qwen → coverage → exact witnesses/status → audit/finish → news enforcement. Исходные labels одинаково ambiguous/background; retrieval и initial classifier не запускались. Бюджет на группу/ветку: 3 reader pages, до 3 Qwen, до 2 generative, 180 с и 24 000 source chars на модельный вызов. Все 8 групп-веток завершились без ошибок.
5. Выполнен настоящий **release CLI `extract-file` для всех 12 HTML**. У 11 успешных страниц `text_hash` совпал с baseline; у P11 воспроизвелась та же ошибка. Отдельно прошли 5 stdio-тестов настоящего release MCP-процесса. Firecrawl подключён только тестовым Rust HTTP-адаптером к тем же consumer-функциям; нового публичного MCP reader пока нет.

Все 16 OpenRouter-ответов указали `deepseek/deepseek-v4.1-flash-1017` и `finish_reason=stop`. Все 23 reranker-ответа указали `Qwen3-Reranker-0.6B-4bit`; оценено 82 + 107 = 189 пар, fallback отсутствует. Число пар посчитано по `pool_ranges` и сверено с реальными `response.results`, а не с сокращённым preview `candidate_pool`.

## Задержка и ресурсы

Основные p50/p95 рассчитаны по **11 общим успешно обработанным страницам × 3 повтора = 33 наблюдения на ветку**; p95 — nearest rank. Полные 36 попыток на ветку, включая три baseline-ошибки P11, также сохранены. Firecrawl reader включает private-network HTTP, проверку ответа и Rust-сериализацию; baseline — локальный Rust extract. Сохранение диагностических файлов исключено из обоих таймеров. Все времена warm: контейнер уже запущен, startup/download не включены.

В первоначальном capture запись candidate response JSON на диск ошибочно входила в его таймер. После заморозки reader/quality outputs изменена только граница таймера и сделан отдельный повтор `latency-capture`: ещё 36 локальных uploads, **без новых target/model calls**. Хеши текстов совпали с первоначальными. Исходный capture, версия harness и `latency-amendment.json` сохранены; quality labels не пересчитывались и не подбирались.

Время последующей Qwen/coverage/audit-стадии на тройку документов составляло 16,24–27,50 с у baseline и 11,39–26,99 с у Firecrawl. Это один проход с удалённой LLM; объявлять Firecrawl быстрее по этим колебаниям нельзя. Медиана reader выросла примерно в 11,9 раза, но абсолютная разница около 30 мс здесь мала относительно модельной стадии. Более содержательный расход — дополнительные 25 Qwen-пар.

Проверенный local-only `/parse` заработал с **API + Redis**, без browser, PostgreSQL и RabbitMQ. API ограничен 2 CPU / 2 GiB, Redis — 0,5 CPU / 128 MiB. Измеренный API memory peak — **393,27 MiB**, поздний usage 256,7 MiB; Redis 4,26 MiB. Это память конкретного sequential HTML-upload режима, а не требования полного scrape/JS stack или VPS под нагрузкой. Размер pinned API image — около 2,56 GiB без сжатия.

## Версия, изоляция и неудачные попытки

Upstream source / image `BUILD_SHA`: `7d8e92dc5d270b1247d60e809c3fd32f52e0e47b`. Официальный образ закреплён manifest digest `sha256:d09e210a9fbbf1fffb6ec8972f23e854498cdee672ee374dbdce9c344a301d8e` (arm64 digest `sha256:a877a49e2ba1715e7ee084044cf61571f38726dd03c4dd1734e63753bf093b7a`). Конкретный endpoint, Compose, image inspect и source probes лежат в архиве. [Предварительная оценка Firecrawl](firecrawl-assessment-2026-11-05.md) описывает состояние до этого опыта.

Docker-сеть эксперимента `internal`, без внешнего IPv4 default route и внешнего IPv6 route, DNS API направлен на loopback. Опубликованных host ports нет; клиент подключался к принадлежащему этому контейнеру private IP OrbStack. В Firecrawl не передавались ключи, cookies, настройки моделей и облачные провайдеры. Браузер не запускался; внешние страницы не загружались; публичный DNS/IP/redirect шлюз Rust не менялся. Синтетический upload URL `parse.firecrawl.dev/uploads/Pxx.html` явно не считается исходным URL или доказательством HTTP-редиректа: логический URL берётся из frozen snapshot manifest, raw bytes связаны хешем.

До основного capture сохранены неудачи установки: отсутствующий image tag, исправление Redis entrypoint под `cap_drop`, отсутствие host port на internal network. Первый synthetic smoke вернул 200 и корректный rawHtml, но Markdown экранировал underscore-маркер. До обработки реальных страниц carrier заменён на cleaned HTML плюс нашу обычную plain-text сериализацию; неудачная проба сохранена. Это сравнение выбранного default main-content HTML режима, не исчерпывающий подбор Firecrawl options.

Всего было **74 локальных `/parse` вызова**: 2 synthetic smoke + 36 исходных + 36 повторов задержки. Отдельно 16 разрешённых OpenRouter generations и 23 local Qwen inference calls; поисковых/target HTTP/browser loads — 0. Эти 74 uploads не являются 74 обращениями к сайтам. Новых провайдеров LLM или внешних scraping API нет.

Оба временных контейнера и их сеть удалены после опыта. Все 8 ранее работавших контейнеров продолжают работать; функциональная приёмка чужих приложений и отсутствие их рестартов этим не доказаны. Кэш загруженного image оставлен; Docker prune не выполнялся. Runtime flags/defaults, клиентские настройки и зависимости приложения не менялись.

## Воспроизводимость и проверки

Долговечные артефакты:

- [Метрики, постраничная сохранность, TP/FN/FP](../feasibility/firecrawl-results-2026-11-05.json).
- [Корпус и сырые captures](../feasibility/firecrawl-corpus-2026-11-05.json): исходные HTML, gold, protocol/freezes, все reader/LLM/Qwen responses, CLI outputs, исправленные и первоначальные timings, диагностика, setup/teardown evidence. Это gzip+base64 content-addressed bundle с SHA-256 каждого файла; создан после измерений и содержит исходные pre-inference freezes.
- [Execution summary](../feasibility/firecrawl-execution-2026-11-05.json), [test harness](../src/firecrawl_benchmark.rs), [offline analyzer](../scripts/analyze-firecrawl-2026-11-05.py), [restore](../scripts/restore-firecrawl-2026-11-05.py), [cohort preparation](../scripts/prepare-firecrawl-2026-11-05.py).

Офлайн-пересчёт из чистого **нового** каталога; без Docker, ключей, сети и моделей:

```sh
python3 scripts/restore-firecrawl-2026-11-05.py runs/firecrawl-offline-review
python3 scripts/analyze-firecrawl-2026-11-05.py --run-dir runs/firecrawl-offline-review --latency-dir latency-capture --output runs/firecrawl-offline-review/recalculated.json
```

Выполненное восстановление и пересчёт дали побайтово идентичный JSON результата, SHA-256 `d932d262c6eabda628e85eb1beaa72bcff1865ed701f5ef21595dc4b8e546052`. Прямой отдельный пересчёт raw labels/sets/pair counts также совпал; это собственная повторная проверка, не независимая acceptance-review.

Имена реально вызванных отдельных ignored-тестов: `firecrawl_benchmark::local_parse_smoke`, `firecrawl_benchmark::capture_paired_readers`, `firecrawl_benchmark::compare_final_direct`. Harness требует явно доверенных startup env и свежих output directories; вся серия `--ignored` не запускалась. Публичного сетевого теста эти имена не разрешают.

Обычные проверки: `cargo fmt --check`; locked/offline unit+stdio **121 passed** (116 unit + 5 stdio); `cargo clippy --locked --offline --all-targets -- -D warnings`; release build; ещё 5 stdio-тестов со свежим release binary. Новый unit self-check отклоняет invalid upload ID и подменённый rawHtml; Python analyzer/restore имеют assert self-checks. Existing production functions, Cargo.toml/Cargo.lock, quality raw captures, исходные gold/original20/B17 не изменились. Из Rust добавлены test-only модуль и его `#[cfg(test)]` declaration.

Router refactor-review вызван на конкретных файлах, но без Git вернул пустой diff; это не семантическая проверка. Фактический diff с исходной копией сохранён и проверен отдельно. Verification и итог связаны с `TASK-20261105-101353-001`, finding — с `RESEARCH-20261029-124155-001`. Формальная независимая `ready_to_merge` acceptance отсутствует; собственные зелёные тесты её не заменяют.

Контекстные lint/index/index-check и реальный stdio Router smoke прошли. Doctor: **WARN, 15 OK / 5 WARN / 0 FAIL** — Git ignore/tracking не подтверждены без Git, есть кандидаты вне source globs и нет ready/open backlog. Схемы, активные ссылки и свежесть индекса проходят; Git или globs ради зелёного doctor не изменялись. В task сохранён `completion: partial` из-за отдельного независимого acceptance gate, а не из-за ожидаемого разрешения пользователя или незавершённых измерений этой серии.

## Решение и границы

Оставить текущий reader основным. Firecrawl остаётся кандидатом для отдельного ограниченного fallback при ошибке извлечения; доказанного выигрыша final direct здесь нет, автоматическое внедрение не выполнено. Потеря P01 показывает, что вместе с более полным body нужно оценивать отбор passages и полноту evidence-контекста, а не только длину извлечённого текста.

Не проверены live URL fetching, публичный Chromium/JS egress, `/scrape` и полный upstream stack, полный поиск с initial classifier, повторная модельная устойчивость, свежий holdout, VPS и нагрузка. Для этих выводов нужна отдельная квалификация; текущая выборка не объединяется с прежними NEW6/NEW12/original20 score.
