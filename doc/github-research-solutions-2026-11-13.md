# GitHub: аналоги для четырёх research P1

Проверены код и тестовые сценарии семи публичных репозиториев. Наиболее полезны механизмы Instructor, Haystack, RAGChecker и ALCE. Их можно проверить в существующем Rust-контроллере без подключения этих библиотек. Это продолжение [плана P1](research-backlog-plan-2026-11-13.md), а не реализация или доказанный прирост качества.

Дата: 13 ноября 2026. База документации a8dab92, runtime40fa7f9 неизменён. Источники прочитаны через GitHub connector; сохранены запросы, ответы, деревья, метаданные и содержимое файлов. Код проектов не запускался, зависимости не устанавливались, внешние тесты/CI не проверялись. Новых продуктовых/model/Harness проб нет. Отсутствующие custom research-playbooks не заявляются выполненными.

## Что действительно стоит проверить

### G1. Instructor — ограниченный repair после ошибки валидатора

[Retry controller](https://github.com/567-labs/instructor/blob/aeb7e2558166caeec497e541bcfc6b4eaa2cf53b/instructor/v2/core/retry.py) ловит parse/validation errors, сохраняет failed attempts и проверяет бюджет перед reask. [Тесты бюджета](https://github.com/567-labs/instructor/blob/aeb7e2558166caeec497e541bcfc6b4eaa2cf53b/tests/v2/test_retry_budget.py) содержат exact-boundary stop и unavailable-usage cases; [тесты изоляции](https://github.com/567-labs/instructor/blob/aeb7e2558166caeec497e541bcfc6b4eaa2cf53b/tests/v2/test_retry_isolation_contract.py) проверяют, что retry не меняет повторно используемую историю caller. Это прочитанные тесты, не наши результаты их запуска.

Наш кандидат для G07: конкретная ошибка поля → максимум один repair в общем `self.repaired` → повторный semantic audit. До repair проверить остаток времени/генераций и финальный reserve; при нехватке сохранить хорошие claims. Сначала сравнить с более простым согласованием plain-text guidance. Не переносить Tenacity, registry или новые публичные budgets. Важное отличие: тест Instructor допускает успешный ответ, пересёкший token budget; наши жёсткие ограничения определяет собственный контракт.

**Не копировать citation validator:** [пример citations.py](https://github.com/567-labs/instructor/blob/aeb7e2558166caeec497e541bcfc6b4eaa2cf53b/examples/validators/citations.py) проверяет лишь вхождение строки в переданный chunk; без context проверка пропускается. Уникальность, subject и entailment он не доказывает. Наши exact ranges/ambiguity guard обязательны.

### G2. Haystack — восстановление формата и сохранение исходного запроса

[JsonSchemaValidator](https://github.com/deepset-ai/haystack/blob/5487d837d3848c197451ff9ec455a856e87bfda4/haystack/components/validators/json_schema.py) возвращает отдельный validation_error с failing JSON, ошибкой и путём поля/schema. Это конкретный образец feedback для format repair, а не основание ослабить safe_claim_text. [Тесты валидатора](https://github.com/deepset-ai/haystack/blob/5487d837d3848c197451ff9ec455a856e87bfda4/test/components/validators/test_json_schema.py) проверяют ошибки и custom recovery message.

[QueryExpander](https://github.com/deepset-ai/haystack/blob/5487d837d3848c197451ff9ec455a856e87bfda4/haystack/components/query/query_expander.py) сохраняет исходный query при сбое генератора, пустом ответе и invalid JSON; очищает/дедуплицирует расширения перед обрезкой. [Тесты](https://github.com/deepset-ai/haystack/blob/5487d837d3848c197451ff9ec455a856e87bfda4/test/components/query/test_query_expander.py) прямо покрывают эти ветви, duplicates-before-truncation и excess-query truncation.

Наш кандидат для документации: исходную форму резервировать в прежнем лимите двух forms, вторую использовать для docs-варианта; дедупликация до расходования слотов. API identifier, версия и site host/path должны сохраняться детерминированно. В чужом коде original query может добавляться после expanded queries: простое взятие первых двух потеряет original. Поэтому не копировать порядок/дефолтное число expansions и отдельный LLM этап; использовать существующий planner. Проверить error fallback, оба одинаковых запроса, excess list, site/path, version и non-doc/news controls.

### G4. RAGChecker — различать поиск факта и использование найденного

[computation.py](https://github.com/amazon-science/RAGChecker/blob/6091f08c00e676e87a970f2aeb4a23a484746348/ragchecker/computation.py) отдельно вычисляет claim recall в retrieved context и context utilization в ответе; [metrics.py](https://github.com/amazon-science/RAGChecker/blob/6091f08c00e676e87a970f2aeb4a23a484746348/ragchecker/metrics.py) фиксирует разные необходимые матрицы entailment. [meta_eval.py](https://github.com/amazon-science/RAGChecker/blob/6091f08c00e676e87a970f2aeb4a23a484746348/data/meta_evaluation/meta_eval.py) сравнивает модельные оценки с human labels, но это не независимый reviewer нашего снимка.

Наш кандидат для F02/F03/F12/G02/G08: строка на обязательное условие с флагами body/selected/draft/admitted/audited/returned witness. Сначала определить первый разрыв; истинный фон не закрывает условие. Разметку из gold не передавать runtime. Сохранить unsupported/conflicted/missing раздельно: чужой to_bool сводит всё, кроме Entailment, к false. Нового evaluator/model не подключать. Unit tests формул в просмотренном дереве не обнаружены; наличие meta-evaluation data не доказывает их корректность на наших случаях.

### G6. ALCE — оценивать ответ и конкретные ссылки раздельно

[eval.py, compute_autoais](https://github.com/princeton-nlp/ALCE/blob/246c476a4edfc564266b7346b6e29ef4861ae937/eval.py) отклоняет отсутствующие/выходящие за верхнюю границу ссылки, проверяет entailment объединённых cited passages и вклад отдельных документов. [Human evaluation](https://github.com/princeton-nlp/ALCE/blob/246c476a4edfc564266b7346b6e29ef4861ae937/human_eval/README.md) разделяет поддержку всех фактов предложения и вклад конкретной цитаты.

Наш кандидат для приёмки: reviewer видит исходное обязательное условие, returned quote/range и достаточный scope; truth, completeness, точность witness и citation relevance размечаются отдельно. Добавить случай «факт верен в другом месте, возвращённая цитата относится к другому API». Чужой NLI работает с passages, а не нашими Unicode ranges: использовать весь документ для спасения плохого witness нельзя. NLI score не заменяет отдельную независимую приёмку.

## Дополнительные находки и ограничения

- **G3 DocPrompting:** [retriever/eval.py](https://github.com/shuyanzhou/docprompting/blob/b7e0e626d47a0649090465bb15c91192fee8777f/retriever/eval.py) позволяет считать retrieval recall отдельно от генерации. Однако calc_recall даёт 1 при пустом oracle; для наших no-evidence/unknown cases такой перенос недопустим. Fixed docs pool не проверяет живой SERP/site filtering. Это справочный код, не готовая замена retrieval.
- **G5 RefChecker:** [aggregator.py](https://github.com/amazon-science/RefChecker/blob/1df1b25cee792ba2b171302e31ca4f768bd67703/refchecker/aggregator.py) различает Entailment/Neutral/Contradiction/Abstain, но strict_agg не обрабатывает Abstain отдельной ветвью. Перед переносом нужно проверить допустимый domain входа; majority vote не подходит нашему no-false gate. Репозиторий архивный на дату чтения. [checker_base.py](https://github.com/amazon-science/RefChecker/blob/1df1b25cee792ba2b171302e31ca4f768bd67703/refchecker/checker/checker_base.py) объединяет результаты по segments/passages — это не exact witness binding. Новую зависимость не выбирать.
- **G7 RAG-query-rewriting:** [исходные rewrite prompts](https://github.com/xbmxb/RAG-query-rewriting/blob/6bab6243e60c7f3e526b6ee44f95381b1024d471/generate/inprompts/myprompt.jsonl) полезны как пример отдельной поисковой формулировки. Но там entity QA/Wikipedia/web и разделители вывода; нет доказательства сохранения нашего site/path/version. RL/training, Bing API и чужие модели не входят в адаптацию. В metadata лицензия не определена; исходники не копировались.

## Снимки и проверяемость

| ID | Репозиторий | Прочитанный commit | Архивный | Лицензия metadata |
| --- | --- | --- | --- | --- |
| G1 | [567-labs/instructor](https://github.com/567-labs/instructor) | `aeb7e2558166caeec497e541bcfc6b4eaa2cf53b` | нет | MIT |
| G2 | [deepset-ai/haystack](https://github.com/deepset-ai/haystack) | `5487d837d3848c197451ff9ec455a856e87bfda4` | нет | Apache-2.0 |
| G3 | [shuyanzhou/docprompting](https://github.com/shuyanzhou/docprompting) | `b7e0e626d47a0649090465bb15c91192fee8777f` | нет | Apache-2.0 |
| G4 | [amazon-science/RAGChecker](https://github.com/amazon-science/RAGChecker) | `6091f08c00e676e87a970f2aeb4a23a484746348` | нет | Apache-2.0 |
| G5 | [amazon-science/RefChecker](https://github.com/amazon-science/RefChecker) | `1df1b25cee792ba2b171302e31ca4f768bd67703` | да | Apache-2.0 |
| G6 | [princeton-nlp/ALCE](https://github.com/princeton-nlp/ALCE) | `246c476a4edfc564266b7346b6e29ef4861ae937` | нет | MIT |
| G7 | [xbmxb/RAG-query-rewriting](https://github.com/xbmxb/RAG-query-rewriting) | `6bab6243e60c7f3e526b6ee44f95381b1024d471` | нет | не определена |

Instructor перенесён из instructor-ai в 567-labs: GitHub вернул redirect, затем проверено каноническое имя. Первые широкие repo searches дали слабые demo-проекты; они не включены как доказательства. Далее проверены известные исходные реализации и связанные с прошлым обзором авторские репозитории. Это целевой отбор, не исчерпывающий рейтинг GitHub и не оценка по stars. Неархивный статус не означает активную поддержку: даты снимков и metаdata сохранены отдельно.

[Машиночитаемые источники](../feasibility/github-research-solutions-2026-11-13.json), локальный raw corpus и checksum — в [manifest](../feasibility/artifact-manifest.json). Сохранены также неинформативные regular prompts и первоначальный redirect; их не выдаём за rewrite-механизм.

## Изменение плана

1. Format P1: добавить budget-boundary, reserve, history-isolation и field-error feedback controls; сначала простой plain-text guidance, repair только если нужен.
2. Docs P1: original query slot + одна constrained docs-форма, dedup до лимита, deterministic site/path/version guards и fallback без дополнительных calls.
3. Coverage P1: отдельная stage matrix на обязательное условие; empty-gold/unknown не считать recall success; не смешивать contradiction с отсутствием опоры.
4. Acceptance P1: раздельная разметка truth/completeness/returned witness; два источника сами по себе не доказывают поддержку; оценщик и reviewer — разные роли.

Все четыре задачи остаются open/planning с прежними acceptance criteria, checks и dependencies. Подтверждён результат поиска аналогов, но эффективность переноса требует будущих сравнений из основного плана. Runtime, release и исторические FAILED gates не изменились.

Проверки этого дополнения: `npm run context:index`, `npx --no-install project-context lint --json`, `npx --no-install project-context index --check --json`, `npm run context:check`, `npm run context:doctor`, `npx --no-install project-context backlog-graph --json`, `git diff --check`.
