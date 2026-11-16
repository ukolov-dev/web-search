# Потери новостных статей на coverage — 3 ноября 2026

**Две старые статьи восстановлены на этапах coverage/audit; роста direct recall в новом полном поиске не наблюдалось.** В новом 8-query A/B с Qwen обе версии дали 3 верных direct, 0 ложных и 0 unknown direct. Предварительно заданный returned-union recall — 3/3→3/3; дополнительная диагностика с двумя прочитанными, но не возвращёнными подходящими статьями — 3/5→3/5 (60→60%). Эти два новых пропуска происходят на проверке даты до coverage. Новые FP не обнаружены, общая квалификация не заявляется.

В opt-in `request-evidence-v12-coverage` исправлен отказ всего coverage-пакета из-за корректных, но лишних свидетельств в незадействованных полях. Новостная подсказка также требует содержательную цитату из тела вместо повторения заголовка. Две прежние подходящие статьи проходят повтор coverage, реальный audit и прежние date/type gates на восстановленных исходных фрагментах. Это отдельная проверка этапов, не полный повтор старого поиска.

## Что изменилось

Четыре transport checks сохранены. При валидном доверенном новостном окне поля после `1 + anchors.len()` не являются условиями запроса. Такие поля приводятся к `missing/[]/""` только после проверки статуса и собственных passage IDs. Для лишнего `supported` требуется точная цитата длиной 16–240 символов из snippet/body/publication_date этого источника. `contradicted`, чужие/неизвестные/повторные ссылки, выдуманные цитаты и title-only свидетельства остаются ошибкой. Диагностика `ignored_unused_news_check` публикуется только после успешной проверки всего пакета.

Лишнее поле никогда не становится доказательством обязательного условия. Активные core/anchors, heading-copy guard, литералы, тип страницы, дата публикации, source status/conflicts и semantic audit сохраняются. Неновостной путь и запросы без валидного окна сохраняют прежний строгий контракт. Модельная подсказка дополняется только для доверенного новостного контекста: сохранить все условия полного вопроса, выбрать реальное body-свидетельство, оставить лишние slots пустыми, не угадывать дату.

Runtime менялся только в [evidence.rs](../src/evidence.rs) и [model.rs](../src/model.rs). [coverage_replay.rs](../src/coverage_replay.rs) — тестовый модуль с одним явно запускаемым ignored replay. Обновлены версия stdio и две regression fixtures. Новых зависимостей, API, повторов генерации или чтения нет. Default legacy сохранён; v12 включается прежним `--evidence-memory`, Qwen — `--reranker-settings`.

## Две старые статьи и причинность

В [предыдущем отчёте](news-navigation-2026-11-03.md) две статьи NN03 были пригодны по исходному тексту и дате, но остались background с `evidence unused requirement`. Публичный MCP не сохранил исторический raw model response. Поэтому он **не объявляется восстановленным**. Восстановлены все 11 фактически переданных passages и доверенные часы по исходным full-body hashes, UTF-8 ranges и полям T/S/H/D; проверка целостности прошла.

На этих точных фрагментах выполнена новая development-проба. Первый узкий ремонт, разрешавший только дату/дубликат активного свидетельства в лишнем поле, оказался недостаточен: модель записала туда другие точные факты тела. Этот неудачный вариант и все его результаты сохранены. В том же ответе C022 повторяла заголовок в core; C023 имела реальное body-свидетельство.

| Повтор на старых источниках | Новые generative calls | C022 | C023 |
| --- | ---: | --- | --- |
| Узкий первый ремонт, новый coverage | 1 | background | background |
| v11, тот же сохранённый ответ | 0 | background | background |
| Финальная нормализация, тот же ответ + реальный audit | 1 | background: heading core | direct |
| Финальная нормализация + новостная подсказка, новые coverage/audit | 2 | direct | direct |
| v11, уже сохранённый корректный ответ последней строки | 0 | direct | direct |

Последняя строка важна: строгий старый валидатор тоже принимает корректный canonical ответ. Восстановление обеих статей относится к сочетанию ремонта, подсказки и вариативности модели; нельзя приписывать весь эффект одной нормализации. На финальных повторах date/type gates проверены вновь. Всего development replay потребовал четыре новые генерации; HTTP/search/Qwen — ноль, использованы прежние реальные Qwen ranges. Это один издатель и один событийный кластер, не два независимых подтверждения.

Регрессионные тесты дают red→green на одном и том же сохранённом входе. Отрицательный audit, missing обязательной даты, invalid core, app advertisement и неподтверждённые extra quotes не превращаются в direct. [Scoped bug report](../outputs/news-coverage-2026-11-03/bug-reproducer-report.md) доказывает этот дефект, не квалифицирует весь поиск. [Все попытки](../runs/news-coverage-2026-11-03/old-replay-summary.json), [проверка восстановленных источников](../runs/news-coverage-2026-11-03/safety/nn03-context-integrity.json), [peer safety review](../runs/news-coverage-2026-11-03/safety/revision2-peer-review.md).

## Новый полный поиск

[Восемь запросов](../feasibility/news-coverage-cases-v1.json), [план](../feasibility/news-coverage-plan-v1.json), исходники, обе release-бинарные версии и инструменты оценки зафиксированы до inference. Baseline — v11, candidate — v12. Два постоянных MCP-процесса, чередование порядка веток, тот же DeepSeek `deepseek/deepseek-v4.1-flash-1017` через разрешённый OpenRouter и локальный Qwen3-Reranker-0.6B-4bit. Новых rewrite/fallback форм нет. Выполнены все **16/16** primary calls, оба процесса завершились с кодом 0. Код после начала нового набора не менялся.

Шесть запросов ищут текущие городские новости; отдельно проверены архивный ноябрь 2025 и будущая дата 3 ноября 2030. «Сегодня» — строго 3 ноября 2026 Europe/Moscow, без периода — 1–3 ноября. Все восемь формулировок новые относительно прежних новостных опытов, но города, издатели и страницы могли повторяться: это query-heldout, **не URL-heldout**. Источники live; одинаковые HTML/model inputs для двух веток не замораживались.

Grade2 означает отдельную статью по месту/теме/периоду с полным оригиналом и собственной полной датой публикации. Ленты — grade1; чужая тема/география/дата — grade0; недостающий оригинал — unknown. Дата события, modified, copyright и соседний teaser не заменяют публикационную дату. Неподходящий direct означает нарушение этой news-rubric, а не ложность каждого факта статьи.

Direct recall считается на общем множестве известных grade2 case/URL, возвращённых **хотя бы одной веткой**, только для пар с двумя выполненными вызовами. Положительная страница, отсутствующая в другой ветке, остаётся её FN. Unknown не попадают в denominator и показаны отдельно; denominator 0 означает `null`, а не 0%. Это условный recall найденного объединения, не recall всего веба.

## Результаты

По заранее заданному правилу выполнены 24 отдельных original-validation запроса через прежний guarded Rust HTTP-клиент: 24 HTTP200, 24 извлечённых текста, ошибок нет. Это дополнительная проверка источников после primary, не скрытое расширение четырёх чтений. Заблокированный andro.io и поддомены исключены; исходно неудачные URL не повторялись. Сохранены raw HTML, метадата и body hashes. HTTP200 сам по себе не засчитывался как подходящая статья.

Два оценщика той же команды получили source cards и raw originals без arm, model labels, audit и runtime date verdicts. Разметка заморожена до соединения с predictions, без исправления grades/schema после join: **76 unique case/URL, 26 grade0, 42 grade1, 3 grade2, 5 unknown**. Карточные 0/1 частично предварительны (`source_reviewed=false`); все grade2 проверены по полному оригиналу и собственной Article publication metadata. Gold SHA256 `0b03817e0b701defa9b7b119017057890381519bacea0aba816ced4fc87cec73`.

| Запрос | Grade2 в возвращённом объединении | Верный direct v11→v12 | Ложный direct v11→v12 |
| --- | ---: | ---: | ---: |
| nc01: Воронеж, сегодня | 0 | 0→0 | 0→0 |
| nc02: Самара, сегодня | 3 | 3→3 | 0→0 |
| nc03: Нижний Новгород, последние 3 дня | 0 | 0→0 | 0→0 |
| nc04: Белгород, сегодня | 0 | 0→0 | 0→0 |
| nc05: Ростов-на-Дону, сегодня | 0 | 0→0 | 0→0 |
| nc06: Самара, сегодня, site:sgpress.ru | 0 | 0→0 | 0→0 |
| nc07: Казань, ноябрь 2025 | 0 | 0→0 | 0→0 |
| nc08: Самара, 3 ноября 2030 | 0 | 0→0 | 0→0 |
| **Всего** | **3** | **3→3** | **0→0** |

По 75 returned rows в каждой ветке; условный primary direct recall **100→100%, delta 0 п.п.**, FN 0→0 на denominator 3. Current-news группа даёт тот же результат; в архивном/будущем контролях denominator 0 и recall `null`. Unknown direct 0→0; новых известных ложных direct IDs нет. Scoped safety gate оценён и прошёл: все 16 captures, полная разметка returned union, нет нарушений модели/бюджета/news policy и unknown candidate direct. **Пройденный safety gate не означает прирост полезности или общий zero-FP.**

Три положительных результата — Samaratoday [автограф Черномырдина](https://samaratoday.ru/news/473479), [приговор экс-сотрудникам СИЗО](https://samaratoday.ru/news/493733), [уход главы Самары](https://samaratoday.ru/news/493709), own `datePublished=2026-11-03`. Это три сюжета одного издателя. Историческая дата документа в первой статье не подменяет дату публикации новости о находке.

### Статьи, которых нет в primary denominator

Чтобы returned-union recall не скрывал потери за пределами top10, оценщики отдельно, всё ещё без predictions, проверили 14 из тех же 24 оригиналов, отсутствующих в source cards; новых HTTP-запросов для этого не выполнялось. Два пригодны для nc03: [поезд с остановкой Нижний Новгород-Стригино](https://nnovgorod.bezformata.com/listnews/poezd-svyazhet-kazan/165476847/) и [диспетчерский пункт на Ракетной](https://nnovgorod.bezformata.com/listnews/punkt-dlya-otdiha/165482058/). У обоих собственное `NewsArticle.datePublished=2026-11-03` в главном блоке страницы, связанное с её URL/заголовком и подтверждённое видимой датой 03.11.2026; полный body подходит запросу. Борская криминальная новость не включена по city-only критерию; два события Bezformata имеют общего downstream-издателя «Время Н».

После заморозки этой source-only оценки raw traces подтверждают: **обе версии прочитали обе страницы (HTTP200), но date parser вернул `publication_date_not_verified`; статьи не дошли до coverage и top10.** Расширенное exploratory множество известных положительных read+returned страниц: denominator 5, TP 3→3, FN 2→2, **direct recall 60→60%**. Это post-hoc диагностика с более широким denominator; первоначальный план/primary score и gold не менялись. Она также не является recall всего веба.

У нескольких Bloknot-оригиналов содержательное тело, но только относительные «сегодня/вчера» без полной собственной даты. Полный год не выведен из серверного времени, query, изображений или соседних teaser; такие возможности остаются с неопределённой датой. Остальные рассмотренные невозвращённые страницы имеют неверное место/тему/период либо недоказанную дату. После этих находок код на новом наборе не донастраивался.

Два отдельных пересчёта raw captures + frozen source gold совпали с анализатором; второй участник не импортировал runner/analyzer для вычисления. Это проверка текущей команды, не формальная независимая приёмка. Данные: [primary results](../feasibility/news-coverage-results-2026-11-03.json), [source gold](../feasibility/news-coverage-gold-2026-11-03.json), [read misses и auxiliary source notes](../feasibility/news-coverage-read-misses-2026-11-03.json), [independent metrics review](../runs/news-coverage-2026-11-03/benchmark/independent-metrics-review.json), [runner](../scripts/news-coverage-benchmark-2026-11-03.py), [анализатор](../scripts/analyze-news-coverage-2026-11-03.py).

## Проверки и пределы

| Все 8 primary queries в каждой ветке | v11 | v12 |
| --- | ---: | ---: |
| Source search calls | 16 | 16 |
| Generative calls | 10 | 10 |
| HTTP document reads | 32 | 32 |
| Qwen calls / passage pairs | 3 / 7 | 3 / 7 |

На каждом вызове соблюдены 3 generative / 4 reads / 4 Qwen / 180 с и 24000 source chars. Все 16 initial classifiers завершились валидно. Максимум source context — 18810; самый длинный primary call — 39,1 с. Суммы wall time 132,0 и 153,3 с описывают этот live-прогон, не доказывают ускорение/замедление версии. Денежная стоимость публичным MCP не возвращается.

Qwen/coverage/audit фактически понадобились только на nc02, по три Qwen-вызова в каждой ветке. Остальные семь запросов использовали лишь initial classifier; после news/date admission подходящих целей coverage не осталось. На новом наборе ни у одной ветки нет `evidence unused requirement`, а v12 не записала ни одной `ignored_unused_news_check`. Поэтому новый набор проверяет совместимость и безопасность полного пути, но **не измеряет пользу нормализации на новых структурных сбоях**. Причинное свидетельство ремонта — отдельные одинаковые-input regression/replay, а не суммарный новый score.

В primary errors сохранены `unsupported_direct_evidence_or_constraints`: nc05 у обеих веток, дополнительно nc04 и nc06 у v12. Это отклонённые исходные direct-гипотезы, не coverage parse failures; initial prompt/guard код в этой итерации не менялся. Различие таких гипотез может быть вариативностью модели. Пустых/пропущенных вызовов и source-block повторов нет.

Финальный код: **106 unit + 5 stdio = 111 ordinary tests passed**, fmt, clippy `--all-targets -D warnings`, offline release build и пять release-stdio checks прошли. `--ignored` целиком не запускался. Два целевых peer-теста проверили отсутствие обхода active-check/audit границ; review той же команды не является формальной независимой приёмкой.

Сохранены baseline/fixed snapshots, все неудачные development-попытки и setup errors отдельного replay crate. Без Git directory diff и SHA фиксируют фактическую правку; пустой Router diff heuristic не считается semantic review. Исходные 20 запросов и B17 policy не менялись. Ключи, cookies и содержимое настроек не архивировались.

Повтор в другой день, нагрузка, VPS и формальная независимая `ready_to_merge` приёмка не выполнялись. Ограниченная эмпирическая проверка не доказывает глобальный zero-FP. Удаление случайного барьера структурного отказа может открыть прежнюю семантическую переоценку модели — поэтому recall, известные FP и unknown оцениваются отдельно.

## Project Context и воспроизводимость

Task `TASK-20261103-144423-001`, verification `VERIFY-20261103-152635-001`, research revision23. CLI фактически вернула `FINALIZED / completion:partial / taskStatus:in_progress / requiresHumanReview:false`. Исправление и локальная проверка выполнены, но эмпирический прирост на свежем наборе не показан, а формальная независимая приёмка не проводилась.

[Project Context workflow](../.agents/skills/project-context-workflow/SKILL.md) явно требует: «`completion: complete` requires a fresh independently accepted `ready_to_merge` execution run». Собственные тесты, source-only grading и peer review этой команды не выдаются за такую приёмку. Это ограничение статуса записи, не ожидающее разрешение пользователя.

Context lint: 0 errors/0 warnings; свежий index и planning stdio-check (34 tools, 3 scoped packs) прошли. Doctor: 15 OK / 5 WARN / 0 FAIL; предупреждения об отсутствии Git-проверки ignore/tracking, source coverage и ready backlog не скрыты и не являются приёмкой продукта. Итог: [execution summary](../feasibility/news-coverage-execution-2026-11-03.json), [code freeze](../runs/news-coverage-2026-11-03/benchmark/code-freeze.json), [source freeze](../runs/news-coverage-2026-11-03/benchmark/native-source-freeze.json), [post-primary integrity](../runs/news-coverage-2026-11-03/post-primary-integrity.json). Raw runs — доказательства сеанса, не долговечная Project Context memory; итоговые cases/gold/results/misses сохранены в feasibility.
