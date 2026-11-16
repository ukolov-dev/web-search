# Исследование для пилота живого веб-поиска с oMLX

[Реализация research P1 субагентами: V1/V2, проверки и незакрытая приёмка](research-p1-implementation-2026-11-13.md).

[GitHub-аналоги для четырёх research P1](github-research-solutions-2026-11-13.md): код/тестовые сценарии семи проектов, pinned snapshots; без внедрения.

План оставшихся research P1: [порядок работ, 8 первичных источников и проверки](research-backlog-plan-2026-11-13.md) — только planning, без новых runtime-изменений/экспериментов.

Дата поиска: **28 октября 2026 года**. Подобраны **22 научные работы 2021–2026 годов**. Цель — предложить проверяемые улучшения исходного ТЗ, сохранив его локальную архитектуру и ограничения.

**Актуальное ТЗ для разработки — [версия 1.3 на Rust](tz-v1.3.md).** Закреплены текущий local на Mac и последующий server API на Linux VPS, единый код приложения и открытый headless Chromium для поставки. Серверный этап отложен; требования к качеству, лимитам и безопасности сохранены.

**Реализация начата:** [отчёт первого этапа от 28.10.2026](feasibility-2026-10-28.md), [сборка и пробы](../README.md). Rust/oMLX работают; этап пока не принят из-за качества/доступности поисковых источников и неподтверждённой изоляции Chromium.

**Продолжение:** [исправления и дополнительные пробы](feasibility-followup-2026-10-28.md), [доказательства регрессионных исправлений](../outputs/bug-reproducer-report.md). Синтетический сетевой сценарий теперь проходит; публичное чтение ещё не включено.

Обоснования решений — в [обзоре](research-review.md); в конце нового ТЗ приведены научные основания и перечень изменений.

| Документ | Содержание |
| --- | --- |
| [Уникальная привязка цитат V7](research-quote-binding-2026-11-13.md) | F09 red→green; отдельное CONFIRM12 wrong-witness3→0,8/11→8/11; lossG07 блокирует acceptance;155unit+6stdio |
| [Приёмка после запуска Qwen](research-qwen-acceptance-2026-11-13.md) | Свежий NEW12:2/11→8/11;94Qwen/no fallback; H01/H02 восстановлены, F09/A3 wrong witness блокирует acceptance |
| [Диапазоны цитат и технический поиск, 13 ноября](research-followup-2026-11-13.md) | D02/D04 component replay, внутренний выбор исходных Unicode spans; живое чтение RFC не прошло, приёмка открыта |
| [Проверка утверждений research: реализация и A/B](research-gates-2026-11-06.md) | OLD R01/R03, сохранённые провалы NEW12/V1 и CONFIRM12/V2, отдельный FINAL12/V4: aspects8/20→11/20, false0→0, unknown1→0; потериD02/D04 блокируют приёмку, Harness H01 подтверждён/H02 не решён; реализация есть, release нет |
| [Семь Luna: live/offline проверка](luna-seven-search-2026-11-06.md) | 7субагентов,14эпизодов; source/direct/claim/provenance/agent-visible/cost отдельно;1ложное нормативное утверждение research,2выявленных recall gates, cache repeat и typed-date fixtures; runtime не исправлялся |
| [Rust bounded research](research-mvp-2026-11-05.md) | MCP/CLI с единым бюджетом, targeted follow-up, отдельным audit и точными Unicode-цитатами; frozen4+2development, все6partial,15witness placements/11answer ranges, реальный Harness1 безspill; known semantic overstatement и ограничения источников, без общей qualification |
| [Harness: NEW20 и кэш](harness-cache-2026-11-05.md) | Реальный pinned Harness SDK, 20 новых задач в трёх режимах; качество конечных ответов, false direct/unknown, фактические расходы и задержка; ограничения источников и воспроизводимые snapshots |
| [Qwen: сохранение начала статьи и Firecrawl NEW12](qwen-firecrawl-selection-2026-11-05.md) | Selector v3 в прежних3окнах; frozen P01 regression;12других URL, отдельный clock-aligned repeat: фрагменты14/19→17/19, direct8/8→8/8, FP0→0; первый прогон с ошибкой часов и invalid old audit сохранены; задержка и raw corpus |
| [Firecrawl: локальный A/B на 12 страницах](firecrawl-benchmark-2026-11-05.md) | Сохранность фрагментов16/17→17/17, own dates6/6→6/6; direct recall6/7→6/7, FP0→0 с обменом FN; reader p50 2,73→32,40мс; реальные CLI и local upstream parse, исторический component без public JS |
| [Firecrawl: применимость](firecrawl-assessment-2026-11-05.md) | Первичный обзор до локального A/B: self-host/cloud, HTML provenance, Rust-интеграция; внедрение не принято |
| [Собственная дата и news anchors v14](news-dates-2026-11-03.md) | Old2 date admission/Qwen/coverage/audit, own body и city/anchor controls, отдельные NEW12, sensitivity2 и NEW6 без объединения score |
| [Новостной coverage v12](news-coverage-2026-11-03.md) | Две прежние статьи проходят stage replay; новый 8-query A/B с Qwen: условный direct recall 3/3→3/3, FP 0→0, unknown direct 0→0; рост не подтверждён, ветка ремонта на новом наборе не понадобилась; read+returned 3/5→3/5, 2 date misses |
| [Новостная навигация и даты v11](news-navigation-2026-11-03.md) | Feed→article в4read budget, исходные даты/unknown и app-direct guard;7новостныхпар: статьи0→3, верныеdirect0→1, неподходящие8→0; appcontrolblocked, полныйNEW8gateнеоценён |
| [Новости Воронежа: живой тест3ноября](voronezh-news-2026-11-03.md) | Search/ranked на3датированных запросах:0подходящих статей против5страниц reference; короткая фраза лучше находит СМИ, но включает app-direct; discover→extract читает2статьи отдельно от ранжирования |
| [Admission/literals v10 — failed quality gate](admission-literals-2026-10-30.md) | Четыре старых miss восстановлены; новый24/440 сQwen: direct27,78→31,48%, FP0→1, unknown1→1. Gold до inference, всех FN/FP разбор; безопасное улучшение не подтверждено |
| [Фрагменты и coverage v9](passage-coverage-2026-10-30.md) | Разбор11 прежних FN, extraction/diverse Qwen, атомарные условия, локальный fail-closed check; неудачные v7/v8 и финальный новый4: direct recall0→33,3%, FP1→0; малый exploratory набор |
| [Восстановление recall](recall-body-verification-2026-10-29.md) | Безопасное чтение background-страниц, ID реальных цитат, вторая проверка релевантности и новые наборы |
| [Direct coverage v6](direct-coverage-2026-10-30.md) | Полный bounded context, status gate, grounded conflict quarantine; новый8/154 direct recall25→31,25%,FP0→0; старые2FP→0,11FN остаются |
| [Extraction/selection v5](extraction-selection-2026-10-30.md) | Явный publisher body и1 reference read slot; свежий8/149, read/top10 recall84,6→92,3%,direct61,5→61,5%,FP4→2,новыхFP0 |
| [Classifier и полный поиск v4](reliability-full-search-2026-10-30.md) | Свежий8/154, DeepSeek+Qwen: initial2/8→8/8, direct recall6,25→56,25%, FP0→0; обязательный audit в opt-in evidence memory, остаточные FN и extraction gap |
| [Qwen3 passage reranker](reranker-2026-10-30.md) | Исторический component8 и полный12 до v4; passages лучше, direct labels без улучшения; erratum6 length+1 stop-empty |
| [Evidence memory: реализация и A/B](evidence-layer-implementation-2026-10-30.md) | Явный startup-флаг; новые12/201, v1/v2, итог recall27,8→22,2%, FP0→0; legacy default |
| [Слой проверки и память](evidence-layer-memory-2026-10-30.md) | Разбор обхода audit, ошибок судьи и отбора passages; предложение состояния evidence и плана A/B/C без runtime-изменений |
| [DeepSeek и MiMo: benchmark](model-benchmark-2026-10-30.md) | 46 вопросов, 1058 кандидатов, одинаковые inputs; recall/ложные direct/raw guards/body/cost и ограничения |
| [OpenRouter](openrouter-2026-10-30.md) | Явный внешний LLM provider, безопасный key-file, сохранённый oMLX; live-проверка ожидает ключ |
| [Релевантность и извлечение](relevance-extraction-fix-2026-10-29.md) | Grounded conservative classifier, semantic extraction и paired-проверка на новых 12 вопросах |
| [Полная B17 policy и живой отчёт](full-policy-live-2026-10-29.md) | Rust ranked_search, 62 свежих SERP, локальная 9B, ручная оценка 20 вопросов и проверка шести целей |
| [Seznam/Naver в Rust](native-seznam-naver-2026-10-29.md) | Два экспериментальных адаптера, offline B17 replay, границы переноса policy и day2 |
| Рабочий Project Context (local development record; not distributed) | Карта знаний, решения, исследование, proposed backlog и проверка поиска контекста |
| [Context Router v0.11.0](context-router-0.11.0-2026-10-29.md) | Локальное подключение инструмента разработки, проверка релиза, CLI/MCP и ограничения doctor |
| [Навигация и достаточность](docs-evidence-probe-2026-10-28.md) | Новый discover в MCP, локальная 9B-проба, обнаруженная утечка ответа из памяти и экстрактивная защита в диагностике |
| [Исследование надёжности](search-reliability-research-2026-10-28.md) | SearXNG/Bing, отрицательная locale-проба, открытые движки и следующие эксперименты |
| [Пять дополнительных работ](search-reliability-papers-2026-10-28.md) | Достаточность контекста, шум, rewrite, RRF и проверка поддержки цитатами |
| [Проверка реализуемости](feasibility-2026-10-28.md) | Реальные прогоны, результаты и блокеры первого этапа |
| [ТЗ v1.3 — local/server](tz-v1.3.md) | Актуальная редакция: режимы размещения, будущий VPS API и открытый Chromium |
| [ТЗ v1.2 — Rust](tz-v1.2.md) | Предыдущая редакция: Rust-стек, проверка извлекателя и браузерной защиты, сборка и поставка |
| [ТЗ v1.1](tz-v1.1.md) | Предыдущая редакция с включёнными рекомендациями и исходным Python-стеком |
| [Исходное ТЗ v1.0](tz-original-v1.0.md) | Точная копия пользовательского файла; предложения не внесены в оригинал |
| [Обзор](research-review.md) | Выводы, приоритеты, применимость к локальному пилоту и пробелы доказательств |
| [Каталог 22 работ](papers.md) | Названия, авторы, годы, статус источника, ссылки на статьи и PDF, применимость и ограничения |
| [Дополнения к ТЗ](tz-amendments.md) | Формулировки для включения в следующую редакцию, с основаниями и проверками |
| [План экспериментов](evaluation-plan.md) | Базовые варианты, метрики, сценарии отказов и правила оценки |
| [Журнал поиска](search-log.md) | Направления и запросы, принципы отбора, глубина чтения и ограничения обзора |

P01–P22 — устойчивые идентификаторы работ в каталоге. D01–D10 — исходные предложения этапа исследования; их нормативные формулировки сохранены в v1.3. Обзор, дополнения и план экспериментов остаются материалами подготовки; при расхождении с ними для разработки применяется ТЗ v1.3. Их упоминания Python/Trafilatura/Playwright относятся к прежнему стеку. Переход на Rust выполнен по выбору пользователя и сам по себе не подтверждает ускорение.

Проверены первичные публикационные страницы и аннотации; для ALCE, RAGChecker и CaMeL дополнительно прочитаны выбранные разделы полного текста. Для остальных работ обзор не означает полного постраничного чтения. Статьи и PDF доступны по ссылкам в каталоге; сами PDF локально не скачивались.

Выполнены начальная Rust-реализация пробника и часть проверок реализуемости; результаты перечислены в отдельном отчёте. Полный сервис и итоговая приёмка ещё не выполнены. Первоначальный литературный обзор остаётся исследованием, а не доказательством достижения показателей.
