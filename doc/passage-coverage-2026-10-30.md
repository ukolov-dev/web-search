# Отбор фрагментов и coverage v9 — 30.10.2026

На последнем новом наборе v9 повысила direct recall **0→33,3%** (2/6 известных релевантных источников), false direct **1→0**, новых FP ID и unjudged direct нет. Проверены4 запроса/70 карточек. Выборка мала; bootstrap95%0…+50п.п. включает0. Это наблюдаемое улучшение полного pipeline, а не общая гарантия или изолированный эффект reranker.

Разобраны все11 прежних FN, сохранены неудачные v7/v8 и исправлена причина JavaFP. Финальная v9 прошла75 ordinarytests и реальный release MCP. Ниже — причины потерь, изменения, протокольное отклонение gold timing и полный журнал измерений.

## Задача и границы

Разобрать11 FN прошлого v6, исправить extraction/passage selection/coverage и проверить конечный **direct recall**, сохранив контроль ложных direct. Новые изменения включены в существующий opt-in `--evidence-memory`; default legacy и runtime Rust сохранены. Local Qwen3-Reranker-0.6B-4bit используется в обеих ветвях. Генеративная модель — закреплённый `deepseek/deepseek-v4.1-flash-1017` через уже согласованный OpenRouter transport.

## Все11 прежних пропусков

Это retrospective development-разбор [прошлого набора](direct-coverage-2026-10-30.md), а не новая независимая оценка. Source witnesses, HTML hashes, UTF-8 offsets и старые Qwen ranks сохранены в [диагностике](../feasibility/passage-coverage-misses-2026-10-30.json). Результат повторного прогона — в [development summary](../feasibility/passage-coverage-development-2026-10-30.json).

| Источник | Причина потери v6 | Итог atomic development |
| --- | --- | --- |
| c01C009 Git cherry-pick | Полезный источник в pool, но вне4 reads; snippet неполон | Не запускался повторно; document selection остаётся |
| c02C005 CPython dataclasses |128 символов навигации вместо полного publisher-declared RST | **Direct восстановлен**, исходный файл извлечён |
| c03C005 Rust Rc | Уже supplied B1/B2/B3; compound anchors создавали лишние gaps | Coverage принял, final audit отклонил связь с strong count |
| c03C018 Rust Rc mirror | Аналогичная ошибка compound anchors | Coverage принял, final audit отклонил |
| c04C005 PostgreSQL | После корректного initial downgrade источник вне pool/reads | Не запускался повторно; selection остаётся |
| c05C014 SQLite | NOT NULL/default в Qwen rank5/6; старый top3 потерял условия | Фрагменты восстановлены; audit требует дополнительную связь с existing rows |
| c06C005 Docker | Три ARG-окна; ENV persistence в rank5 потерян | Фрагменты восстановлены; audit не принял все условия |
| c07C011 CSS issue | Нужный абзац в rank5 и compound anchor gaps | В новом development не прочитан |
| c07C015 Shopify CSS | Snippet обрывался до объяснения Tab; страница не читалась | **Direct восстановлен** |
| c07C017 CSS | Нормативный traversal-order фрагмент в rank5 | **Direct восстановлен** |
| c08C019 HTTP421 | Полная норма в snippet; `non-idempotent` ошибочно считался API literal | **Direct восстановлен** |

В шести одинаково исполненных старых c-кейсах known TP3→6 из12, direct recall25→50%, known FP0→0. Появился1 unjudged direct c02C006: прежний gold был unknown, его не меняли после предсказания. Из исходных11 FN восстановлены4, пять сохраняются, два не запускались повторно. Одновременно потерян прежний TP c03C007 на semantic audit. Это не утверждение об устранении всех11.

Два прежних ложных direct проверены отдельно: n01C020 Git stash и n07C015 unsatisfied CSS proposal остаются background. Текущие source-status/conflict guards сохранены.

## Что изменено

1. **Publisher-declared code source.** Extraction понимает единственный typed `code-view` payload с совпадающими file paths, полными `rawLines` и проверенной line count. Только полный видимый нетранкированный файл; ошибочный/множественный payload возвращается к прежнему извлечению. Строки HTML-экранируются и не исполняются. Реальный retained CPython RST:128→34857 символов, восстановлены `asdict`, recursion, `copy.deepcopy`. Размер сам по себе не критерий качества: exact source witnesses и text hash проверены.
2. **Взаимодополняющие Qwen-фрагменты.** Первое окно остаётся rank1 Qwen; ещё два выбираются из top6 по новым query features, с прежним ограничением overlap<50%. Подсказки ранжирования не являются доказательством поддержки. Один scoring call/страницу, до3 окон, UTF-8 ranges и capped128 windows сохранены. На frozen scores прежних positive retained witnesses21→24/28, потерь0; это component-метрика, не direct recall.
3. **Атомарные условия coverage.** Up to3 anchors — целые уникальные single-word query tokens вместо составных перекрывающихся фраз. Runtime проверяет целый токен, а не подстроку (`After` не заменяет `Retry-After`). Полный исходный запрос остаётся обязательным R0; каждый direct всё ещё проходит отдельный semantic audit. Exact quotes, IDs, source hashes и4-check shape сохранены. Неиспользованные checks обязаны быть missing.
4. **Естественное отрицание.** Некавыченный lowercase `non-X` проверяется как смысловое условие исходного запроса. Quoted/backticked names, CLI flags, CamelCase, цифры/underscore/дополнительный дефис сохраняют literal guard. Общий legacy matcher не менялся. Audit по-прежнему обязан проверить само отрицательное условие.
5. **Audit guidance.** Полная нормативная фраза в snippet может быть substantive evidence. Нельзя требовать лишний tutorial или выдумывать implementation how-question по keyword query. При этом каждое материальное условие должно поддерживаться этим же источником; соседний API или другой candidate не заполняют gap.

### Отклонённый вариант

Первый development-вариант оставлял только общий R0. Он дал **новый FP c02C018**: статья описывала рекурсию, но не подтверждала deepcopy. Он отклонён, его outputs и source snapshot сохранены. Финальный atomic-вариант этот источник не принимает. Этот неудачный опыт входит в стоимость и ограничения отчёта; он не скрыт как успешная абляция.

## Первый новый A/B: v7, отклонённый результат

Новый заранее сохранённый набор —12 запросов и231 retrieved cards,24 logical search calls Seznam/Naver. Темы: pathlib, git reset, iterator bounds, PostgreSQL RLS, SQLite UPSERT, recursive readonly bind mounts, CSS sticky, HTTP304, async lru_cache, sparse-checkout, CTE и Arc clone. Исходные20 и B17 не изменены.

Baseline v6 и fixed v7 используют одинаковую сохранённую SERP, один raw acquisition cache, каждое тело извлекается собственным кодом ветви. Отдельный baseline binary собран из исходного snapshot; проверяются versions и binary hashes. Initial classifier запускается в обеих ветвях, bypass запрещён. Бюджеты одинаковые:3 generative/4 reads/4 rerank calls/180с/24k context. Failed/blocked source snapshots переиспользуются без повторного обращения; исторические блокировки сохранены.

Код v7 зафиксирован **до первой baseline-генерации**, без последующих изменений runtime. Source-only автор размечает original HTML, не видит predicted labels и model outputs; exact quotes/hashes/UTF-8 ranges проверяются автоматически. Gold фиксируется перед анализом результатов, после inference. Это blind annotation одним автором, не независимая приёмка и не исчерпывающая разметка веба. Неполные источники получают grade1, неизвестные остаются unjudged; grade1 direct считается FP.

В gold-only plan добавлен один retrieved p03C002 по источниковой неполноте Iterator/ExactSizeIterator, до просмотра labels. Эти дополнительные source reads не попадают в inputs ветвей. Критерии all-material не менялись ради повышения recall.

## Новый полный результат

Первая фиксированная проверка:12 запросов,231 карточка,64 known/18 positive/167 unknown. Frozen gold показал **TP12→10, FP2→0, FN6→8**, direct recall66,7→55,6%. Unjudged direct0 в обеих ветвях. Bootstrap95% разницы−31,25…0 п.п. При одинаковых16/18 прочитанных positive и17/18 returned-top10 рост финального direct не получен. V7 не объявлена улучшением recall.

После раскрытия результатов автор обнаружил одну ошибку аннотации: p12C005 `Clone` содержит поздний абзац об Arc/Rc, shared data и increment count. Frozen gold не переписан. Source-grounded erratum и отдельная sensitivity: baseline13/19=68,42%,FP1;v7 10/19=52,63%,FP0. Поэтому снятие двух frozen FP нельзя выдавать за два доказанных ложных direct: подтверждён один p06C010, второй был true source. [Диагностика потерь и erratum](../feasibility/passage-coverage-v7-diagnosis-2026-10-30.json).

Основные потери: p10C009 был отменён из-за title-only core другого кандидата; p12C004/C005 — из-за трактовки `deep` как истинной предпосылки; p11C007 имел поддерживающий verdict с положительным объяснением в поле `missing_aspect`, нарушая строгий output contract. Остальные ограничения — не прочитанные sources, literal namespace и завышенные требования audit. Для p05 и p08 не нашлось полного known positive: timeout/неполные доступные источники; эти вопросы не создают искусственные positive.

### Поправки v8 и ещё один отклонённый development-результат

Title-only core теперь понижается прежним witness validator до ambiguous локально; неподтверждённый источник не получает direct, остальные сохраняют свои checks. Exact двухкандидатная регрессия101→0. Неизвестные refs/IDs, неправильный контракт и незавершённый output не принимаются.

Coverage поясняет, что свойство в вопросе может быть ложным: явное доказательство обратного поддерживает отрицательный ответ, сохраняя R0 и audit. Audit schema в evidence mode задаёт `supported=true` только вместе с пустым `missing_aspect`; runtime validator остался прежним. Общий legacy path не меняется.

Development на раскрытых6 случаях восстановил p10C009 и p12C004/C005, но ошибочно повысил p11C004: blog excerpt перечислял SQL-опции, не объясняя направление их эффектов. Ошибка подтверждена отдельным source review; gold не менялся. Добавлено общее требование source-связи каждой обязательной альтернативы с эффектом. В отдельной p11-проверке этот FP снят; true direct там тоже не восстановлен. Неудачный development output сохранён и включён в стоимость.

## Второй новый8: v6 против v8

Новый набор зафиксирован до acquisition/inference: Go channel receive, Linux MAP_PRIVATE/msync, C++ vector reserve, Java computeIfAbsent, MySQL foreign_key_checks, Kubernetes probes, MongoDB updateMany, .NET ToList. Пересечений с183 историческими query rows не найдено. Всего8 запросов/140 карточек/16 logical search calls. Все случаи сохранены, независимо от доступности sources.

V8 зафиксирована до первой baseline-генерации этого набора. Бюджеты, model pin, Qwen, общий raw acquisition и provenance checks те же. Source-only автор сначала готовит draft gold; второй агент вслепую проверяет полные источники для positive и partial до окончательного freeze. FINAL predictions до freeze не анализируются. Это дополнительный контроль source-аннотации, не формальная independent acceptance.

| Метрика | V6 + Qwen | V8 + Qwen |
| --- | ---: | ---: |
| Known TP / FP / FN |2 /0 /8 |3 /1 /7 |
| Direct recall |20% |30% |
| Unjudged direct |0 |0 |
| Прочитанные positive |8/10 |8/10 |
| Returned top10 positive |10/10 |10/10 |
| Initial valid |8/8 |8/8 |

Появился **новый FP f04C004**: статья неверно утверждала, что null computed result в `computeIfAbsent` вызывает NPE. Поэтому v8 не прошла требование no-new-FP. Frozen gold27judged/10positive/113unknown подтверждён вторым blind reviewer до анализа. У f05/f07 полного known positive нет: blocked/timeout и отсутствие явной interleaving-связи. CI прироста0…30п.п.; этот рост не принимается как безопасное улучшение.

## V9: восстановление полного reference-контекста и проверка witness

Ошибка f04 разобрана по исходному HTML и фактически переданным windows: Oracle reference C005 потерял весь method detail в Readability (8707chars), Android C003 window оборвался перед явным returns-null. Одна более слабая фраза `unless null` присутствовала, но полный self-contained counter-rule отсутствовал. Нельзя выдавать этот случай исключительно за ошибку parser или исключительно за аудитор.

- Узкий Javadoc selector использует согласованные keywords/class header/contentContainer description/summary/details/method-detail markers, без domain/API constants и без выбора по длине. Oracle retained HTML даёт **8707→65087chars**, atomic invocation/null-result contracts восстановлены. Малформированные, множественные и несогласованные структуры возвращаются к прежнему пути; cap/hash/coverage guards сохранены.
- Secondary Qwen windows сдвигаются к сильному exact operation-heading/signature, сохраняя прежние query features и1200-char slot. Первый ranked slot остаётся прежним; следующая операция не подмешивается. Реальный C003 сохраняет Returns/Throws; frozen12-source replay24→24 witnesses, loss0. Произвольное prose-упоминание не создаёт heading.
- Audit теперь видит **фактическую coverage-цитату каждого условия**, а не только passage IDs. Контекст привязывает subject/qualifiers, но не заменяет неверный witness другой фразой. Добавлены роли input argument/computed result/stored state.
- Data fixes и quote forwarding сами по себе ещё не сняли JavaFP. Для финального audit того же model pin включён `low` reasoning с6000 output-token budget и `exclude=true`. Initialnone4096/coverage none6000,3generative/4reads/4Qwen/180с/24k source context сохранены. Это пакет изменений, а не изолированный эффект reranker. [OpenRouter](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens) предупреждает, что thinking делит max_tokens с visible output; поддержка low проверена GET/models и фактическим usage1354reasoning tokens на development.
- Первый reasoned development обнаружил конфликт, но выдал одну malformed quote и валидный дубликат. Known-target пары теперь quarantine обоих даже при плохом witness; только точные пары публикуются как grounded, валидные дубли объединяются, неизвестные IDs/shape остаются глобальным отказом. Чужой независимый валидный verdict сохраняется. Никакая невалидная цитата не становится доказательством.

Финальные5 development-контролей n01/n07/p06/p11/f04: known FP0, unjudged direct0; C004 Java отвергнут, C003 принят. Stash конфликт консервативно понизил и корректный источник — это цена quarantine, а не выбор истины по domain. Source/condition/quote guard tests имеют exact red101→green0. Исторические development-проверки и все неудачные outputs сохранены.

## Последний новый4: v6 против v9

До inference зафиксированы четыре новых API-вопроса: Python rstrip/removesuffix, Redis EXPIRE options, Object.freeze shallow/strict behavior, POSIX dup/dup2 descriptor state. Пересечений с191 прежними query rows не найдено; rename заменён на dup до freeze из-за совпадения с прежним сценарием. Низкая выдача не служит причиной замены вопроса.

Код v9 зафиксирован до первой baseline-генерации нового4; после этого runtime не менялся. Один source-only автор и blind peer проверяют positive/partial по полному raw HTML до gold freeze; predicted labels до freeze не анализируются. Gold-only plan не участвует в inputs поиска. Последовательные12+8+4 — exploratory engineering series; первые два набора после раскрытия стали development, результаты всех стадий опубликованы. Последний4 слишком мал для калибровки или общего вывода о вебе.

| Метрика | V6 + Qwen | V9 + Qwen |
| --- | ---: | ---: |
| Initial valid / reasoning tokens |4/4 /0 |4/4 /0 |
| True direct / false direct / FN |0 /1 /6 |**2 /0 /4** |
| Unjudged direct |0 |0 |
| **Direct recall** |**0%** |**33,3%** |
| Precision среди known direct |0/1 |2/2 |
| Вопросы с true direct |0/4 |2/4 |
| Positive в coverage pool / top10 |5/6 |5/6 |
| Positive реально прочитаны/извлечены |3/6 |4/6 |
| Генерации / чтения |11 /13 |11 /14 |
| Qwen calls / pairs / fallback |10 /148 /0 |11 /161 /0 |
| Проверенные direct proof ranges |5 |10 |
| Reported primary cost |$0,001698466 |$0,002003762 |

**Новых FP ID нет; единственный baseline FP h02C010 удалён.** Valkey EXPIRE не доказывает запрошенный Redis scope. Gold70cards:14 judged,6 positive,56 unknown; все returned direct оценены, unjudged direct0. По два full positive у h01/h03/h04; h02 без known positive, официальный Redis недоступен, доступные Godis/Redict/Valkey не закрывают исходный scope/все options. Naver capture h02 вернул `parser_changed`; повторов/подмены вопроса не было.

| Новый вопрос | Positive | TP v6→v9 | FP v6→v9 |
| --- | ---: | ---: | ---: |
| Python rstrip/removesuffix |2 |0→1 |0→0 |
| Redis EXPIRE options |0 |0→0 |1→0 |
| Object.freeze shallow/strict |2 |0→1 |0→0 |
| POSIX dup/dup2 |2 |0→0 |0→0 |

Это наблюдаемый прирост на **четырёх запросах/шести известных релевантных retrieved sources**, не web recall и не answer accuracy. Descriptive query-bootstrap95% **0…+50п.п.**, включая0. Один gain h01C012 связан с изменившимся initial order/read при одинаковых initial inputs — это вариативность генерации, а не изолированное доказательство selector. Для h03C001 extracted hash и набор окон совпадают: v6 audit выдумал требование throws при nested mutation, v9 корректно принял источник. Pure reranker uplift из полного A/B не следует.

**Протокольное отклонение:** confirm-plan требовал gold до inference. Фактически source gold frozen после обеих arms, но до просмотра predictions автором/helper/peer/root. Queries/criteria/code зафиксированы до inference; timestamps и frozen plan не переписаны. Blind review33 source witnesses выполнен до unseal. Это exploratory verification, не заранее зарегистрированная независимая acceptance.

Четыре FN: h01C003 в pool на9-й позиции, но не прочитан; h04C008 понижен SERP literal grounding и не попал в coverage pool; h03C012 имеет все4 supported checks, но блокируется `literal:JavaScript`; h04C006 также полностью covered, но `literal:FD_CLOEXEC` не принимает нормативный close-on-exec alias. Следующий практический bottleneck — document selection и типизация literal qualifiers/aliases. Расширять смысловые aliases без нового precision-контроля нельзя.


## Проверки и ограничения

Для финальной v9 пройдены75 ordinary tests:70unit+5stdio, fmt, all-targets clippy с `-D warnings`, release build и5 stdio проверок свежего release. Четыре детерминированные регрессии extraction/reranker/negation и title-only сосед имеют exact red101→green0; для whole-token atomic guard также сохранены failed/passed логи. [Нативные отчёты](../outputs/passage-coverage-2026-10-30/blob-report.md), [reranker](../outputs/passage-coverage-2026-10-30/reranker-report.md), [negation](../outputs/passage-coverage-2026-10-30/negation-report.md) относятся к компонентам, не к качеству всего поиска.

Сохраняются ограничения: часть источников не достигает4 read slots; model audit способен переусердствовать с completeness и отвергнуть полезный source. Конфликты вне audit targets не проверяются; audit — та же LLM, не truth oracle. Natural `non-X` heuristic может ослабить exact matching некавыченного строчного identifier; quotes сохраняют строгий режим. Непарный ASCII апостроф в естественном тексте способен, наоборот, вернуть false negative. Этот low-severity recall edge обнаружен review после freeze и раскрыт без изменения fresh-кода.

VPS, MiMo, нагрузка, day2 и общая квалификация пилота не проверялись. Формальная independent ready_to_merge acceptance не выполнялась; собственные tests, source review и gold её не заменяют.

## Артефакты, стоимость и состояние проекта

Все стадии: **262 priced generations, $0,046005858 reported provider cost**, ещё9 MCP generations без опубликованной цены. Всего271 generation, включая отклонённые варианты, development и3 release MCP controls. Новых API providers/поисковых API, глобальных конфигов, VPS или automatic hooks не добавлено. Исходные20 SHA256 `b67e3269830ae0e976aa9b4c5dc2f6b9c3e922ad1adda35738289d6b1b8325d6`, B17 и net/search/policy/rescue/mcp guards сохранены.

Финальный release MCP v9 реально прошёл `ranked_search`: statusok, errors[],3generative/4reads/4Qwen/61pairs. Для предыдущего v7 smoke был partial из-за безопасного initial grounding downgrade; этот результат тоже сохранён. Все ordinary/stdio проверки финальной версии пройдены; source review — внутренний peer review с disclosure авторства, не formal independent acceptance. Router task `TASK-20261030-184909-001` финализирована partial по этому acceptance gate; запрошенные реализация, измерения и отчёт доставлены.

- [Оставшиеся4 FN и два gains](../feasibility/passage-coverage-confirm-misses-2026-10-30.json).
- [Новый4 и frozen plan](../feasibility/passage-coverage-confirm-cases-v1.json), [gold](../feasibility/passage-coverage-confirm-gold-v1.json), [metrics/per-case](../feasibility/passage-coverage-confirm-results-2026-10-30.json), [durable corpus](../feasibility/passage-coverage-confirm-corpus-2026-10-30.json).
- [Execution/cost всех стадий](../feasibility/passage-coverage-confirm-execution-2026-10-30.json), [final FP controls](../feasibility/passage-coverage-v9-controls-2026-10-30.json).
- [Первый12: v7 results](../feasibility/passage-coverage-results-2026-10-30.json), [v7 corpus](../feasibility/passage-coverage-corpus-2026-10-30.json), [losses и annotation erratum](../feasibility/passage-coverage-v7-diagnosis-2026-10-30.json).
- [Второй8: v8 results](../feasibility/passage-coverage-final-results-2026-10-30.json), [v8 corpus](../feasibility/passage-coverage-final-corpus-2026-10-30.json).
- [Нативные component regressions](../outputs/passage-coverage-2026-10-30/README.md); реальные before/after captures, source snapshots, executed-src, code/gold freezes и CLI logs — в соответствующих `runs/passage-coverage*2026-10-30`.

Source selection, global literal rules и same-model semantic judgement по-прежнему ограничивают recall/precision. Два returned true direct и0 observed FP не дают общей гарантии. Default legacy сохранён; v9 остаётся экспериментальным opt-in. Пилот и source qualification16/20 в два разных дня не закрыты.


Project Context: lint0errors/0warnings, indexfresh, planningMCPsmoke passed; doctor {'ok': 15, 'warn': 5} (WARN,0fail). Отсутствие Git не позволяет подтвердить ignore/tracking; Router refactor-review heuristic не заменяет actualdiff/semanticreview.
