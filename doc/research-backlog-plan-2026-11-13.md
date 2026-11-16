# План четырёх P1 research и первичные референсы

Запрошенный этап планирования завершён: уточнены четыре существующие задачи, порядок работ и критерии выбора решения. Это литературно обоснованные кандидаты, а не подтверждённые исправления. Runtime не менялся, новых продуктовых/model-проб не было. Дата обзора: 13 ноября 2026.

## Исходное состояние и границы

Проверяемая база: runtime `40fa7f9`, backlog `28bea4e`. [CONFIRM12 V7](research-quote-binding-2026-11-13.md) сохранил 8/11 полных factual-вопросов, убрал wrong-witness 3→0, но потерял G07: приёмка FAILED. Это отдельный набор от [первого F01–F12](research-qwen-acceptance-2026-11-13.md). G07 сначала расходовал чтения на посторонние страницы, затем прочёл Rust docs; draft с `Option<(&str, &str)>` отклонён `safe_claim_text`. Uniqueness guard в этом эпизоде цитату не отвергал. Общая причина F02/F03/F12/G02/G08 пока не установлена.

Неизменяемые ограничения будущей реализации: 2 формы, 4 SERP, 4 чтения, 4 Qwen, 6 генераций, 180 секунд, 24000 source chars, ответ до 32 KiB; trusted startup, локальный Qwen, прежние source-state/cache/URL/DNS/IP/redirect guards. Не добавлять API, модели, зависимости или настройки ради этих гипотез. Фактический ledger берётся из `source_state_after` [последнего результата](../feasibility/quote-binding-confirm-results-2026-11-13.json), не сбрасывается между arms. Отказы продуктовых источников не перепроверялись.

## Порядок и диагностическая развилка

Рекомендуемый порядок: **формат → документация → покрытие → независимая приёмка**. Первые три задачи технически независимы, приёмка уже зависит от всех трёх. Новых backlog-дубликатов и циклов не создавать.

До исправления каждого провала сохранить для обязательного условия цепочку: query → SERP candidate → прочитанный body → selected chunks → draft → application validation → точный witness → semantic audit → итоговый missing/status. Исправлять первый доказанный разрыв. Раздельно считать наличие факта в полном body, в выбранном контексте и в возвращённой цитате. Переданные оракулом URL допускаются только для component replay: они не доказывают успешный retrieval.

## 1. Формат G07

Задача: сохранять подтверждённый ответ при техническом типе (local development record; not distributed).

1. Воспроизвести исходный draft и chunks через `safe_claim_text` → `prune_invalid_claims` → `Session::work` в `src/research.rs`. Раздельно проверить JSON/schema repair в `Session::generate`: сейчас он не восстанавливает claim после application-level text rejection.
2. Первый кандидат — согласовать DRAFT_PROMPT и существующий plain-text контракт, описывать технические типы словами; не разрешать произвольные `<`/`>` и не удалять символы вслепую из готового ответа.
3. Только если этого недостаточно в сравнении: проверить один format-only repair с конкретной ошибкой валидатора на том же исходном контексте. Переиспользовать общий `self.repaired`, прежний лимит генераций и резерв финального audit; после нового claim заново провести semantic audit. Новый цикл/лимит/публичный параметр не нужны. Если бюджета нет — сохранить хорошие claims, оставить аспект missing.
4. Проверки: исходный G07; разнообразные вложенные Rust types, Unicode, numeric arrays/ranges; HTML/links/reference injection как negatives; повтор цитаты и wrong API subject; сохранение соседнего хорошего claim. Запрещено подменять quote, выбирать первый неоднозначный match или специализировать исправление под `Option`.

Основание: [S1 JSONSchemaBench](https://arxiv.org/html/2501.10868v3) раздельно оценивает покрытие схем, корректность и стоимость constrained generation; [S2 JSON Schema](https://json-schema.org/draft/2020-12/draft-bhutton-json-schema-validation-01) описывает структурные ограничения. Наш вывод: schema-valid JSON не закрывает локальные text/witness/semantic gates. Эти источники не доказывают пользу нового decoder для нашего startup. [S7](https://proceedings.iclr.cc/paper_files/paper/2024/file/8b4add8b0aa8749d80a34ca5d941c355-Paper-Conference.pdf) обосновывает различение внешней ошибки валидатора и бесконтрольного «подумай ещё»; пригодность repair у нас требует измерения.

## 2. Поиск запрошенной документации

Задача: официальная документация (local development record; not distributed).

1. На сохранённых G07 SERP разделить качество исходной формы (`Session::query/search`), сортировку кандидатов и порядок чтения (`read_round`), затем наличие нужного раздела после selection.
2. Сравнить минимальную API-форму с одной уточнённой documentation-формой в существующих двух forms: сохранять API identifier, язык, явную версию и site host/path. Не расширять заданный site; не вставлять gold URL. Возможное предпочтение docs применять к реальным кандидатам согласно запросу, а не ко всем запросам/новостям.
3. В том же бюджете проверить порядок очереди отдельно от переписывания запроса. Метрики: rank подходящего документа, номер read slot, наличие целевого контракта в selected chunks, затем supported answer. Официальный домен сам по себе не доказывает факт.
4. Контроли: явный site/path, отсутствующая версия, сходные API, обычный ненормативный вопрос, news, blocked host/cache. Не менять strongest Qwen window и опубликованный news selector ради одного docs-примера.

Основание: [S3 Ma et al.](https://aclanthology.org/2023.emnlp-main.322.pdf) рассматривает query rewriting отдельно от frozen reader. В работе есть обучение/RL и другие задачи: переносим только проверяемую постановку, без обучения нового rewriter. [S8 DocPrompting](https://arxiv.org/html/2207.05987v3) разделяет получение документации и использование её генератором. Его fixed documentation pool и NL→code отличаются от нашего живого bounded поиска; результаты не обещают прирост здесь.

## 3. Обязательные факты вместо фона

Задача: покрытие вопроса (local development record; not distributed).

1. Разметить отдельно F02 (204 content/trailers), F03 (no-store), F12 (обязательность 429), G02 (SHOULD уникальности JSON имён), G08 (panic Vec::insert). Наличие правильной фоновой фразы не закрывает нужное условие.
2. Переиспользовать `plan.facets`, facet IDs и audit missing: обязательное условие закрывается целиком поддержанным claim с нужным subject/condition, а не числом claims. Отделить generator miss от selection miss до изменения prompts.
3. Если факт присутствует в chunks, сравнить guidance обязательных условий; если отсутствует — target фактически missing facet через имеющиеся draft/audit next и оставшийся round. Не добавлять отдельный LLM этап. Если опоры нет, сохранить missing/partial.
4. Если диагностика укажет на позицию/обрезку: отдельный component контроль с тем же содержимым, перестановкой позиций и прежними 3 slots/24000 chars; без увеличения окна. RFC literal policy, SHOULD/MAY/MUST, отрицание, Unicode ranges, версия и source scope сохраняются.

Основание: [S4 RAGChecker](https://arxiv.org/html/2408.08067v1), §§3–4 и Appendix H, разделяет claim recall retrieval и использование контекста генератором; его английский benchmark и автоматический entailment не заменяют нашу разметку. [S5 ALCE](https://aclanthology.org/2023.emnlp-main.398.pdf), §§3.2–3.3,6 и Limitations, оценивает корректность и citation quality отдельно: модельная проверка цитат остаётся ошибающейся. [S6 Lost in the Middle](https://aclanthology.org/2024.tacl-1.9.pdf) даёт протокол контроля позиции доказательства; эффект на наших моделях/коротких chunks не установлен.

## 4. Приёмка после исправлений

Задача: независимая приёмка (local development record; not distributed).

1. До новых inference заморозить снимок кода/бинарника, startup/model settings, вопросы, отдельный gold обязательных условий, raw inputs, оценочную политику и baseline. Известные G/F/H пометить development controls; не называть их новыми.
2. Сначала component replay, затем отдельный fresh paired RU/EN набор и два реальных Harness задания по существующему контракту. Для каждой пары раскрыть общий/разный source set, cache, Qwen scoring/fallback, расходы, attempts с неизвестной стоимостью, отказы и потраченные бюджеты. Cold/warm и разные исторические cohorts не объединять.
3. Независимый reviewer проверяет каждое acceptance criterion по hash-bound bundle и исходному returned witness. Факт в другом разделе не исправляет неверную цитату; оценка той же LLM и корреляция с людьми в статье не дают независимую приёмку нашему снимку.
4. Gate: ноль новых false/unknown-as-cited IDs и ноль потерь каждого baseline supported условия, плюс выполненные критерии всех четырёх задач. При любой потере gate FAILED независимо от среднего score. Общая qualification исходных 20 запросов/двух дней остаётся в прежних задачах; scoped acceptance не заменяет её и не означает выпуск.

Основание: S4/S5 предлагают раздельные метрики и human meta-evaluation, но описывают ограничения оценщиков. S7 требует сопоставлять inference cost: repair не получает скрытые дополнительные вызовы. Это причины для нашего протокола проверки, а не уже пройденная приёмка.

## Первоисточники, версии и отбор

| ID | Проверенный первоисточник | Что прочитано / граница переноса |
| --- | --- | --- |
| S1 | Geng et al., *JSONSchemaBench: A Rigorous Benchmark of Structured Outputs for Language Models*, arXiv:2501.10868v3, 27.02.2025 | HTML полный сохранён; §§3–6: набор схем, coverage, efficiency, reasoning setup. Другая модель/decoder; не проверяет наши semantic/witness gates. Название ранней версии отличается. |
| S2 | JSON Schema Draft 2020-12 Validation, редакция 16.06.2022 | Официальный draft vocabulary; структурные/string constraints, content vs assertions. Это Internet-Draft, не окончательный IETF standard и не оценка LLM. |
| S3 | Ma et al., *Query Rewriting for Retrieval-Augmented Large Language Models*, EMNLP 2023, 5303–5315, DOI 10.18653/v1/2023.emnlp-main.322 | PDF: pipeline, experiments/analysis, Limitations. Landing использует вариант названия «in»; здесь название PDF «for». RL/другие engines/models не переносим. |
| S4 | Ru et al., *RAGChecker*, arXiv:2408.08067v1, 15.08.2024 | Полный HTML: §§3–4, meta-evaluation, Appendix H. Не выдаём v1 за проверенную идентичность camera-ready NeurIPS 2024. |
| S5 | Gao et al., *Enabling Large Language Models to Generate Text with Citations*, EMNLP 2023, 6465–6488, DOI 10.18653/v1/2023.emnlp-main.398 | PDF: claims/citation recall/precision, human evaluation, Limitations. NLI ошибки и неполнота open-ended gold; quote exactness не равна смысловой опоре. |
| S6 | Liu et al., *Lost in the Middle*, TACL 2024, 157–173, DOI 10.1162/tacl_a_00638 | PDF: позиционные controlled changes, §2 setup. Дополнительный диагностический протокол, не основание менять размер контекста. |
| S7 | Huang et al., *Large Language Models Cannot Self-Correct Reasoning Yet*, ICLR 2024 | PDF: intrinsic/external feedback, evaluation setup, cost, scope/limitations. Не доказывает невозможность format repair с точной ошибкой валидатора. |
| S8 | Zhou et al., *DocPrompting: Generating Code by Retrieving the Docs*, arXiv:2207.05987v3 / ICLR 2023 | Полный HTML: §§3–4,6 и top-k ablation. Подтверждённый backward snowball S4 reference57→S8; fixed pool и code generation, не живая квалификация документационного поиска. |

Выбраны 8 первичных источников с разной ролью, не 8 независимых доказательств эффективности. S6/S7 — вспомогательные. S8 найден по реальной библиографической ссылке S4 (Zhou et al., arXiv:2207.05987), затем проверен первичный текст. RefChecker пока не добавлен отдельным кандидатом: для плана достаточно S4, не нужен новый evaluator. Вторичные пересказы, mirrors/forks и abstract-only результаты не использованы как доказательства.

## Воспроизводимость и дальнейшие проверки

Протокол, точные поисковые запросы/ответы, чтения, карточки и ошибки сохранены в локальном raw corpus, указанном в [artifact manifest](../feasibility/artifact-manifest.json); [машиночитаемый список](../feasibility/research-backlog-plan-sources-2026-11-13.json). OpenReview JSONSchemaBench вернул challenge; обхода/повтора не было. NeurIPS PDF RAGChecker дал tool TimeoutError; использован явно помеченный авторский arXiv v1. HTTP 200 сам по себе не считался чтением содержимого.

План проверки **этой документационной задачи**:

```sh
npm run context:index
npx --no-install project-context lint --json
npx --no-install project-context index --check --json
npm run context:check
npm run context:doctor
npx --no-install project-context backlog-graph --json
git diff --check
```

Будущие runtime-команды и поведенческие проверки уже перечислены в каждой P1; выполняются в отдельной реализации. Для живых сравнений сначала записать исполнимые команды и frozen protocol, затем точные команды в verification — описательное название эксперимента не подменяет запуск.

Ограничение процесса: прежняя карта research-playbooks недоступна; пользователь ответил «наверное его уже нет». Отсутствующие custom playbooks не выполнены и не восстановлены в рамках этой задачи. Завершён ограниченный обзор и план, а не экспериментальная проверка подходов. Пересмотр плана нужен при stage diagnosis, опровергающем механизм, регрессии по ID, изменении бюджета/контракта или новых данных от независимого reviewer.

## Дополнение: исходники на GitHub

[Семь проверенных репозиториев](github-research-solutions-2026-11-13.md): bounded validation feedback, original-query fallback/dedup, stage coverage и citation-level review. В четыре P1 добавлены соответствующие контроли и ограничения. Новых runtime-изменений/экспериментов нет.
