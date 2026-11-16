# Research P1: реализация и две отдельные проверки

Три субагента реализовали исправления, затем отдельный reviewer проверил код, исходные цитаты и условия ответов. Реализация есть; **общая приёмка задач и выпуск не завершены**. Последний проверенный runtime — `46f8953`; предыдущий `9b7dc17` сохранён как не прошедшая V1. Ни merge, ни push, ни deploy не выполнялись.

## Что изменено

- [Формат G07](research-format-p1-2026-11-13.md): plain-text guidance и один format-only repair используют прежний общий allowance и резерв аудита; хорошие claims сохраняются. HTML/links, exact Unicode witnesses, ambiguity и RFC gates сохранены.
- [Очередь документации](research-docs-p1-2026-11-13.md): при явном запросе документации реальные кандидаты соответствующего API container получают предпочтение; сохранены site/path и обычные/news запросы. Отдельный review исправил пунктуацию `Rust:`. URL выглядит как документация — это ещё не доказательство официальности или факта.
- [RFC-клаузы](research-coverage-p1-2026-11-13.md): G02 больше не теряется перед отдельным lowercase ABNF-абзацем; уточнены границы диапазонов. Само усиление prompt не восстановило F02/F03.
- [Quote-bound audit](research-quote-audit-round2-2026-11-13.md): поддержка claim проверяется по его exact returned witnesses и явно заданным входам вопроса; прочие chunks — только для опровержения, конфликтов и поиска missing. Для подстановки входных данных требуется inference с rationale.
- [Видимость конфликтов](research-audit-visibility-2026-11-13.md): после обрезки полного chunk нельзя использовать другую скрытую цитату лишь потому, что его ID нужен для mandatory witness. Независимая bounded-регрессия red→green; точный показанный witness остаётся допустимым. Внутренний chunk_id не добавляется в публичный output.
- [Одна система координат для модели](research-coordinate-context-2026-11-13.md): глобальные start/end документа убраны только из генеративных chunks; внутренние и возвращаемые Unicode offsets сохранены.
- [Объявленное RFC тело](research-rfc-body-extraction-2026-11-13.md): canonical URL, номер в metadata, xml2rfc generator и собственные заголовки должны совпасть, после чего исходное тело сохраняется до Readability. У F03 Readability вырезал саму норму no-store:54294→73476 символов без увеличения лимита100000. Namespace extraction cache — extract-v2, старый потерявший секцию текст не переиспользуется.
- [Маркер абзаца `.¶`](research-rfc-pilcrow-2026-11-13.md) распознаётся только перед whitespace/EOF. Нормативная клауза остаётся исходным срезом до маркера; inline continuation и неполные clauses удерживаются.

Новых зависимостей, поисковых API, моделей, публичных параметров или расширения budgets нет. Runtime по-прежнему Rust; OpenRouter/local Qwen задаются только trusted startup. Сырые корпуса, ключи и пользовательские настройки не включены в контекст/артефакты Git.

## Два набора, которые нельзя объединять

| Набор | Baseline → candidate | Обязательные условия | Полные ответы | False | Unknown-as-cited | Gate |
| --- | --- | --- | --- | --- | --- | --- |
| V1:8 новых вопросов в известных семействах +4 known controls |40fa7f9→9b7dc17 |8/16→13/16 |4/12→9/12 |0→0 |2→2 |FAILED: новый G09/A2, хотя потерь условий нет |
| V2:6 новых вопросов в известных семействах +6 known controls |9b7dc17→46f8953 |5/18→12/18 |3/12→7/12 |0→0 |6→1 |Scoped paired no-new/no-loss PASSED; приёмка всех задач остаётся partial |

Числа — независимая проверка каждого обязательного условия и **его собственных returned witnesses**, а не доверие sufficient/semantic verdict модели. Baseline V2 — новый запуск предыдущего бинарника на другом заранее зафиксированном наборе; его нельзя сравнивать с V1 candidate как повтор того же опыта. Положительные условия V2 baseline не потеряны; оставшийся J01/A2 соответствует прежним неизвестным выводам J01/A2+A3, а не новому риску из-за перенумерации/слияния.

V1 G09/A2 получил выдуманную конкретизацию начального вектора и остатка из quote, который показывал только pop==Some(3). Quote-bound component правильно отклонил A2 и сохранил A1. В V2 лишний пример G09 не вернулся. Но J01 всё ещё иногда заимствует общий unwrap_or rule из другого claim, не включая его в собственные witnesses. Это известный незакрытый semantic limitation, не разрешение считать весь ответ безопасным.

Отдельные known Harness H01/H02: в обеих итерациях H01 1/1→1/1 и H02 3/3→3/3. V1 неправильно маркировал конкретные H02 выводы как fact; V2 вернул inference с rationale, использующим входы исходного вопроса. Эти условия не прибавляются к знаменателям12-вопросных наборов. Выполнение SDK-эпизода само по себе не означает правильный ответ.

## Что доказано и что осталось

- **F03 восстановлен в живом V2**: полная response no-store клауза, private/shared и оговорка must-understand привязаны к matching primary RFC9111.
- **F02 восстановлен частично**: запрет content поддержан, trailers остаётся missing. Нужная специальная клауза RFC9110 есть в raw HTML, но находится за прежним100000-character cap извлечённого тела. Лимит не увеличивался; соседний RFC не выдаётся за запрошенный.
- **G07**: format/controller defect исправлен, но общий absent-delimiter контракт не доказан одним cfg example/signature. V2 удержал такие claims. Для закрытия acceptance нужен явный общий контракт/implementation witness в допустимом reading path.
- **G08**: V1 восстановил >len; V2 не получил принятого ответа. Source-aware/model variation сохранена явно; успешный queue replay не означает гарантированную доставку каждого факта.
- **F12**: диагностика отделяет selected evidence от фона; слово optional относилось к428, а не429. Эта первоначальная неверная интерпретация исправлена и сохранена в stage report. Полное восстановление429 optionality не заявляется.
- **J01/A2**: остаётся одна unknown-as-cited конкретизация unwrap_or; quote-bound guidance уменьшил ошибки, но не даёт гарантии исполнения моделью.

Оригинальные20 запросов, два разных дня, общая квалификация engines/9B и production readiness — прежние отдельные задачи. Текущие результаты их не закрывают. Четыре P1 не объявляются выполненными по среднему score.

## Границы экспериментов

Вопросы, gold, binaries и политика были зафиксированы до соответствующих inference; gold/URLs не передавались runtime. Gold V2 независимо проверен до freeze, включая полные raw primary HTML для F02/F03. В V1 digest sanitized settings записан после ранних проб (mtime предшествовал запуску), это ограничение сохранено; V2 startup identifiers/hash зафиксированы заранее. Даты/срезы исторических документов не выдаются за сегодняшнюю доступность всех источников.

Пары имеют разные source sets: V1 одинаковы2/12, хотя общий body есть11/12; V2 одинаковы5/12, общий body9/12. Поэтому это source-aware full-path comparisons, не изолированный model benchmark. Caches раздельны по arms, первоначально пусты; внутри arm есть warm hits. Hit не считается живой HTTP-проверкой. Qwen HTTP calls: V1 cases40→30, V2 cases36→32; fallback0 в обоих наборах. Harness отдельно добавляет4→4 Qwen calls в каждой итерации. Все точные бюджеты/quotes/версии/размеры контекста и equality tool payload проверены; raw source/hash pairs сохранены.

Состояние отказов накопительное через последовательные процессы и обе итерации. Добавлен www.webstaurantstore.com (source_blocked), повторов после отказа нет. Последний ledger — source_state_after [V2 results](../feasibility/research-p1-round2-results-2026-11-13.json). Не сбрасывать его перед продолжением.

Отдельные model components не суммируются с full-path cohorts: suffix-only F02/F03 дали0→0; coordinate-only draft дал0→0 admitted claims (identity/¶/состав контекста ещё мешали); quote-bound G09 audit дал нужный reject A2. Component G09 использует mock-session source metadata вокруг исходного текста, поэтому не доказывает полную живую version verification. Бounded unit/HTML replay не заменяет actual Qwen/retrieval.

## Проверки и артефакты

Runtime46f8953: `cargo fmt --check`; `CARGO_HOME="$PWD/.cache/cargo" cargo test --locked --offline` —170unit+6stdio,37+10 ignored; clippy `--all-targets -- -D warnings`; release build; отдельные6stdio со свежим release — прошли. Только выбранные explicit ignored model probes запущены отдельно и сохранены. Весь ignored-набор не запускался.

Планы/JSON: [V1 protocol](../feasibility/research-p1-protocol-2026-11-13.json), [V1 results](../feasibility/research-p1-results-2026-11-13.json), [V1 independent quality](../feasibility/research-p1-quality-2026-11-13.json), [V2 protocol](../feasibility/research-p1-round2-protocol-2026-11-13.json), [V2 results](../feasibility/research-p1-round2-results-2026-11-13.json), [V2 independent quality](../feasibility/research-p1-round2-quality-2026-11-13.json). Корпус с запросами, ответами, raw HTML/model calls, red/green и журналами — локальный ignored archive в [artifact manifest](../feasibility/artifact-manifest.json). Ключи, SDK homes/configs, binaries и caches исключены.

Независимая criterion-level приёмка записывается отдельно от implementation и command checks. Невыполненные критерии остаются in_progress; отдельная scoped задача может быть принята своим verdict, но последний implementation snapshot не является выпуском. Глобальные настройки/VPS/automatic hooks не менялись; работающий старый MCP-процесс автоматически не перезапускался.
