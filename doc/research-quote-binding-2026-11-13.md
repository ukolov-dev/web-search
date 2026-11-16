# Неоднозначные цитаты research: V7, 13 ноября 2026

**Привязка первого повтора исправлена; приёмка no-loss не пройдена.** V7 больше не выбирает молча первое совпадение текстовой цитаты в chunk. Отдельное CONFIRM12 убрало wrong-witness ошибки3→0, но потеряло поддержанный baseline ответ G07. Одинаковый общий счёт8/11→8/11 не отменяет эту потерю.

Это продолжение [прогона с запущенным Qwen](research-qwen-acceptance-2026-11-13.md): первый F01–F12 остаётся FAILED с F09/candidate/A3. После его полной фиксации F09 стал regression fixture; тот же набор не выдаётся за новое подтверждение исправления.

## Минимальное изменение

Session::quote для chunk_id+quote проверяет, что первый и последний exact match совпадают. Несколько вхождений, включая перекрывающиеся Unicode-подстроки, требуют более длинного уникального исходного среза либо explicit range. При неоднозначности прежний prune_invalid_claims удаляет плохую citation, сохраняет остальные пригодные witnesses/claims и требует прежнего semantic audit. Prompt объясняет необходимость уникального contiguous source context.

RFC selected_start продолжает разрешать точное выбранное вхождение повтора. Новых схем публичного MCP, конфигов, источников, зависимостей, retries или модельных вызовов нет. Бюджеты, quote length12..360, source identity/version, URL/DNS/IP/redirect/refusal guards и legacy ranked gates сохранены. Уникальность сама по себе не доказывает семантическую поддержку; отдельный audit обязателен. Если модель не выбирает достаточно уникальный контекст, claim удерживается — это явное ограничение.

Реальный F09 fixture сначала дал ожидаемый red, после изменения green: неоднозначный O(1) отброшен, правильный ответ про None и пригодный пример сохранены. Добавлен overlapping Unicode test с проверкой explicit range. **155unit+6stdio**, fmt/clippy/release build и отдельные6release stdio прошли. Проверенный src SHA-256 `5923069353fc80325c2a5755a0273c4b75a7030f5ca1cafa658c4ea6fdf0a0ed`, binary `69213f41f86e44cd4ce50775e0da65b0a16e15fd0d3403b65af5091090952752`.

## Отдельное CONFIRM12

[Протокол](../feasibility/quote-binding-confirm-protocol-2026-11-13.json), [cases](../feasibility/quote-binding-confirm-cases-2026-11-13.json), [quality](../feasibility/quote-binding-confirm-quality-2026-11-13.json), [results](../feasibility/quote-binding-confirm-results-2026-11-13.json). Это8новых вопросов и4явно известных контроля, а не12ранее невиденных вопросов. RFC gold подтверждён отдельным MCP extract до freeze; references/gold не передавались inference. Baseline — c42fc3c/V6, candidate — замороженный V7; чередование baseline→candidate, раздельные первоначально пустые cache DB, общий накопительный refusal ledger. Код/prompt/gold после freeze не менялись.

| Метрика CONFIRM12 | V6 | V7 |
| --- | ---: | ---: |
| Полностью закрытые фактические вопросы с корректными returned witnesses | 8/11 | 8/11 |
| Claims | 22 | 26 |
| Фактически ложные claims по прочитанному контексту | 0 | 0 |
| Неподтверждённые возвращённые привязки | 3 | 0 |
| G11: неподтверждённый API не утверждается | pass | pass |
| p50 MCP latency | 11,708с | 17,281с |
| Известный внутренний API-расход за12вызовов | USD0,009044984 | USD0,011367368 |

V6 wrong-witness IDs: G09/A2 иG12/A1/A2. Факт про O(1) имеется в прочитанном контексте, но generic complexity quote выбирает соседний push_within_capacity. V7 в G12 использовала более длинную уникальную цитату сложности pop, включая начало следующего pop_if; фактический диапазон принадлежит pop. Поддержанный контекст в другом месте не подменял проверку returned range.

**Потеря G07:** baseline вернула None при отсутствии delimiter в split_once. Candidate первая форма не сузила site и прочитала посторонние страницы; после достижения Rust docs draft включил `Option<(&str, &str)>`, отклонённый прежним plain-text guard. В G07 нет ambiguous-quote rejection. Пригодный baseline пример остаётся уникальным и допустимым для V7; причинный эффект нового guard на потерю не доказан. Но live whole-policy no-loss gate всё равно FAILED. Ошибка не исправлялась адаптацией к этим G-предсказаниям.

G02(SHOULD уникальности JSON-имён) иG08(panic insert при index>len) не закрыты в обеих ветвях. G09/G10 положительные контроли сохранены; G12 получило правильно привязанный ответ вместо baseline wrong-witness. G11 V7 вернула3верных фоновых факта о capacity Vec, но не утверждала freeze_owned; это не положительный ответ об API.

Source sets совпали1/12, хотя все12пар имели общее прочитанное тело. Это source-aware whole-policy comparison, не изолированная оценка скорости/эффекта guard и не общая qualification.

## Исходные сценарии и доказательства

Отдельный настоящий OpenRouter replay сохранённых D02/D04 прошёл: D02=1claim, D04=3claims, missing=[] для обоих. Это source component, без нового retrieval. Два candidate SDK/Harness задания completed, по1реальному research каждый; agent-visible tool JSON равен inner result, spill0. H01 восстановил1/1условие, H02 —3/3условия в2claims. Оба ответа partial из-за одного цитируемого тела, не из-за отсутствия этих условий. Требование разных тел не выдаётся за независимость издателей.

CONFIRM12 и два Harness вместе: **81реальный Qwen HTTP call, fallback0**. Сохранены26MCPepisodes плюс4generative component calls; известный API-расход **USD0,022652196**, unknown-cost0. Эти числа не объединяются с первым F-набором в quality score. Локальное железо и работа ассистента не оценены.

[Архив и hashes](../feasibility/quote-binding-confirm-package-2026-11-13.json):607members, все hashes проверены; SDK homes/config/workspace, keys, binaries иcache исключены, exact credential scan чист. Сырой архив исключён из Git, внесён в artifact manifest, не standalone installer. Прежние блокировки, включая eur-lex.europa.eu HTTP202, сохранены; новых блокировок в этом этапе нет. Актуальный source_state_after — в новых results.

Реализация V7 локально проверена, но независимая приёмка/ready_to_merge, merge/release/deploy/CI/Linux/VPS не выполнены. Остаток — потеря G07 при whole-policy проверке (retrieval и формат claim), затем отдельное подтверждение без потерь и независимый reviewer. Task/backlog остаются in_progress/partial; исторические FAILED записи сохранены.
