# Живая проверка и выпуск 0.1.0-rc.1

В отдельной новой серии выполнены все12проб на неизменном наборе4историческихNEW+2OLD:6пар baseline/candidate,50успешных LLM responses,33Qwen,46logicaldocumentreads,21SERP. Фатальных ошибок,429,transportretry и not_run нет. Известная стоимость — USD0,009023932. Остановленные прошлые кампании не возобновлялись.

## Качество

| Группа | Baseline | Official-domain candidate |
| --- | --- | --- |
| NEW4: условия | 2/9 | 2/9 |
| NEW4: полные вопросы | 0/4 | 1/4 |
| OLD2: условия | 3/5 | 2/5 |
| OLD2: полные вопросы | 1/2 | 1/2 |
| NEW4: false / unknown-scope claims | 1 / 1 | 0 / 1 |

Gate **FAILED_NO_LOSS**: gainP04.B не компенсирует потериP02.B (порядок/однократность predicateRust) иN04.B (CPythonnotdefault). Приоритет не включён в release runtime. Проверены17/17собственных цитат по sourceSHA и исходным Unicode ranges; точная цитата не равна полной смысловой поддержке. P01baseline цитировал cachetools, не отвечая проfunctools. Для PostgreSQL кандидат поднимал officialpage18 вместо нужного CTE12. Оба варианта сохранили неопределённое обобщение SQLitedefaultOFF; baseline также ошибочно объявил runtime-enable обязательным при любом builddefault.

Статусы nativecontroller:1ok/11partial. Они не равны независимой полноте/истинности ответа: условие двух документов и semanticgold оцениваются отдельно. NEW и OLD не объединялись; это известный повторный development-набор, не freshqualification/original20/day2. Общая квалификация поиска и production readiness не подтверждены.

## Протокол

До всех вызовов заморожены42файла: код/Cargo/driver/gold/6question-only inputs/seed/copiedtestbinary; freezeSHA25676d2f9ce8713fb51e283fd27d3545dc9643748618f1551f712ad17d2160bd77e. Вопросы/язык/gold/модель/порядок/лимиты неизменны. Без подсказанных URL/плана/body. Полный nativeSession: planner→SERP→guardedHTTP/extract→Qwen→draft→audit, а не liveMCPstdio transport. По180с/6генераций/4SERP/4чтения/4Qwen,10с pacing; общийcap72generations,actual50.

KnownfailedURLs PythonGreekglossary/pythonpool оставались в URLexclusions, hardhostrefusals и история сохранены. Три read actions были остановлены URLguard до HTTP. Успешные документы не удалялись после ошибки другого URL. Никакого CAPTCHA/proxy/fingerprint обхода или сброса состояния ради результата.

Modelplanning/SERP/selection стохастичны; накопительные исключения меняют доступность междуarms, во время части проб шла локальная сборка публикации. Время/стоимость описательны, причинность сортировки или ускорение не доказаны. Pacingбез429в50calls не доказывает устранение причины исторических429.

## Выпуск

Runtime исправляет служебный Check.reason без изменения verdict/witness/conflict/two-document gates; rawcapture сохраняет исходную строку. Действуют безопасная диагностика OpenRouter, site/redirect/DNS/IP guards и MCP resource links. Officialpriority и source-awareURLpolicy остаются test-only.

Чистый публичный snapshot исключает приватную историю, инструкции, конфиги, ключи, кэши и сырые корпуса. Publiccheckout: fmt,202unit+7stdio иclippy-Dwarnings прошли;49unit/10stdioignored. Дополнительно7stdio прошли со свежим releasebinary. macOSarm64archive9,027,264bytesSHA2565176a5658060750a8516133961a739d6b5de526416d1a98a08d0710a8d87599e; binarySHA2561220f5872145fde9f795f1bede2cc71d828a6c34e39a60e38d23b92be0a568d3. Архив содержит CLI/BUILD_INFO/README, без пользователя/домашнего пути вmetadata/binary, ключей или весов. MCP initialize сообщает0.1.0-rc.1 и5tools. CLI--version не поддержан; первоначальная неудачная проверка заменена фактическим MCP serverInfo.version probe, не скрыта.

GitHub PR/CI/merge/tag/release и скачивание опубликованного артефакта фиксируются отдельным evidence после выполнения; этот текст не объявляет преждевременно выпуск завершённым. Это experimental prerelease, не серверный VPS deployment. DeveloperID/notarization не предоставлены; иные платформенные binaries не заявлены.

[Независимая quality](../feasibility/live-release-quality-2026-11-18.json), [результаты/актуальный campaignledger](../feasibility/live-release-results-2026-11-18.json), [манифест сохранённых корпусов](../feasibility/artifact-manifest.json). Сырые данные остаются локально и не входят в GitHub release.
