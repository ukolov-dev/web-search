# Исправления и повторная проверка MCP, 18 ноября 2026

Адресные исправления реализованы и Qwen восстановлен, но **критерий улучшения NEW4 не пройден**: покрытие4/9→5/9 при потере двух прежних условий и новом ложном содержании. Кампания остановлена после HTTP429: выполнены15/16попыток. Кандидат не квалифицирован к выпуску.

## Изменения

- `c3809bc`: `site_filter` различает точный Boolean `OR`/`|` и естественный `or`. Строгие host/path/version ограничения, URL/DNS/redirect guards сохранены. Unit и настоящий stdio regression проверяют разрешённый вопрос и отклонённый DSL.
- `1df1f97`, `fd5c6be`, `620c5ec`: общий `navigation_guard` отклоняет заголовки standalone connection-challenge, включая прежний Vimeo body. Заголовок в обычной статье допускается; пустой article в другом месте не скрывает challenge. Captured reduced fixture и отрицательные примеры сохранены. Это узкий detector, не универсальное распознавание любых challenge.
- `45f5dbb`: audit не может закрыть facets старым релизом, если в вопросе ровно одна явная трёхкомпонентная версия, а заголовок cited source явно указывает другую. Отброшенные claims открывают missing заново. Сравнения, title без версии, dates-only и другие грамматики остаются semantic audit; prompts уточняют дату/версию и полный собственный witness. Детерминированная проверка не является общим доказательством истинности.
- Тот же commit добавляет dotted Python API к приоритету документации; сохранены symbol/fragment/language checks. Нет whitelist известных benchmark URLs или увеличения бюджета.
- Local oMLX до запуска возвращал connection refused. Запущено установленное приложение; контрольный `/v1/rerank` HTTP200, релевантный фрагмент0,765625 против нерелевантного0,00002146. Исполнитель не редактировал настройки, автозапуск или сервисы. После запуска приложения mtime settings изменился; без предыдущего полного содержимого нельзя подтвердить неизменность всех настроек. Explicit model IDs и loopback endpoint сохранены в evidence.

Изменения готовили три агента в отдельных worktrees; два других независимо проверяли guards и разметку. Владение файлами согласовано, пересекавшиеся search.rs hunks перенесены узкими cherry-picks. Обнаруженная коллизия общего cargo target отброшена как недостоверный check; достоверные адресные проверки содержат имена новых тестов, интеграционные проверки выполняются на одном итоговом дереве.

## Протокол

[Новый NEW4](../feasibility/search-repair-cases-2026-11-18.json): четыре новых вопроса, девять условий, RU2/EN2, Rust/Python/SQLite, software-only; два Python случая. Gold независимо подтверждён до live. По четыре `research` вызова старого и нового кода с работающим Qwen. Отдельно восемь прежних вопросов /18условий повторяются новым кодом как development regressions. Наборы не объединяются; исходные20 не менялись. No-evidence случая нет, правильный отказ не измеряется.

[Замороженный протокол](../feasibility/search-repair-protocol-2026-11-18.json) переиспользует [старый stdio runner](../scripts/search-five-agents-2026-11-18.py) через [проверяемую оболочку](../scripts/search-repair-2026-11-18.py). NEW4 baseline→candidate, затем old8 candidate; план16attempts; фактически15, последнийB02 не запускался после429. Внешние источники/LLM не менялись; trusted startup DeepSeek через OpenRouter и localQwen, gold не передаётся продукту. Бюджет каждого вызова прежний. Отдельные первоначально пустые кэши фаз, один cumulative refusal ledger. Отказы baseline могут ограничить candidate: порядок не сбалансирован, результаты exploratory.

Vimeo добавлен в начальный ledger на основании уже прочитанной challenge-страницы R01: новый запрос не выполнялся. Все прежние блокировки из предыдущего отчёта сохранены. Повторов после отказа и сброса состояния нет. SERP отдельно не перезапускался: здесь проверяется полный research путь, изменение search касается DSL и покрыто unit/stdio.

Кандидат `620c5ec4fec2ca3f127200a5ad785138fea510ad`, binary SHA-256 `90e17bd097196834aa33ff4233b1dc991bd1a1b487638d8d31a52870ad246977`. Старый binary `2099d8c7ed538ee4e62106923fdbbf27ab29ec36d2060d24aa956ebb3475dbf3`. Freeze сохраняет hashes исходников, driver/cases/gold/startup. Кандидат не менялся после своего freeze. Поле candidate_source_commit в раннем startup фиксировало планfd5c6be; фактический итог620c5ec зафиксирован в отдельных candidate/regression freeze, они определяют проверенный executable.

Отклонение процедуры: baseline уже начал выполняться, когда статическое ревью потребовало сузить article exception (`620c5ec`). Исправление сделано до candidate inference по независимому code finding, без подгонки под оценку ответов. Это явно сохранено в raw protocol-deviation; не выдаём эксперимент за полностью preregistered qualification.

Старое3/18→новый old8 repeat нельзя трактовать как изолированный эффект кода: одновременно восстановлен Qwen. Эффект нового кода оценивается отдельно на NEW4, где Qwen работает в обеих фазах; малый размер и один запуск не дают общей калибровки.

## Результаты и границы вывода

[Фактические вызовы/лимиты/ledger](../feasibility/search-repair-results-2026-11-18.json), [независимая разметка gold и каждого claim](../feasibility/search-repair-quality-2026-11-18.json).

| Отдельная когорта | Поддержанные условия | Полные вопросы | Ошибки |
| --- | --- | --- | --- |
| NEW4 baseline, здоровый Qwen | 4/9 | 1/4 | False N04/A1; unknown3 |
| NEW4 candidate, здоровый Qwen | 5/9 | 1/4 | Новое false content N04/A1; unknown3 |
| OLD8 исторический, Qwen fallback | 3/18 | 1/8 | False F02/A3 |
| OLD8 candidate repeat, здоровый Qwen | 7/18 frozen / 7/16 attempted | 3/8 planned | Новый false T01/A4; unknown0;7/8attempted |

**NEW4 gate FAILED**: потеряN01.A/B, восстановленыN02.A/B иN04.C. Совпадение ID N04/A1 в обеих фазах скрывает замену одного ложного содержания другим: baseline переносит историческое beta-testing в настоящее; candidate заявляет, что free-threaded CPython3.13 не поставляется официально и требует только собственной сборки. Это противоречит primary evidence об официальных Windows/macOS installers. Сравнивались смысл и текст claim, а не только его позиционный ID. Новых unknown propositions не установлено: потери scope installer/framework уже были в baseline, соответствия записаны reviewer.

N01 старый код доказывал eager/lazy различие Rust Option, новый вернул пустой ответ. N02 старый код завершился source_query_mismatch (с misleading stop_reason=model_error), новый доказал оба условия str.strip. N03 в обоих случаях закрывает per-connection, но собственная цитата не доказывает явно отсутствие ошибки внутри transaction: no-op без no-error — неполное условие. N04 у кандидата2/3: experimental и отдельный free-threaded build, но общий default не доказан. Статус `ok` сам по себе этого не исправляет.

**OLD8 — отдельный development repeat**, не оценка чистого эффекта кода. T01 теперь2/2, T02 —3/3, F01 —2/2. В T01 лишний A4 безусловно требует предварительную сортировку, хотя группировка последовательных ключей работает без неё. Ранее A3 корректно ограничивал сортировку задачей собрать все равные ключи: соседняя правильная фраза не оправдывает ложное обобщение. R01 возвращает год2003 без exactday —0/2; R02 —0/2. F02 не дошёл до проверки версии из-за invalid research plan —0/3, это не доказательство исправления на полном пути. Для version guard есть отдельная offline red→green регрессия.

B01 получил настоящий **OpenRouter HTTP429** и пустой финальный ответ. Кампания немедленно остановлена, B02 **not_run**, повторов нет. Ранее B02 inputvalidation исправлен и покрыт stdio, но live coverage этого случая не проверена. Отсутствующие условия не являются unknown returned claims или правильным смысловым отказом.

36/36 MCP Qwen actions успешны, fallback0; отдельно выполнен один local health probe. Проверены35/35 исходных Unicode quote slices, answer ranges, hashes/версии witness, совпадение text/structured/capturedresult, budgets2/4/4/4/6, 24000sourcechars и32KiB structured result. Это provenance/протокольные проверки, а не гарантия правильности утверждений.

NEW4 p50 baseline32,825с / candidate34,400с; OLD8 repeat31,043с по семиattempts. Известная стоимость успешных зафиксированных model calls: USD0,011748252. Failed429 не содержит usage: стоимость этого обращения неизвестна. Не считать его нулём.

## Проверки и оставшаяся работа

Адресные регрессии подтверждают naturalor/explicitOR, exactsitepath, capturedchallenge/benignarticle и emptyarticleoutside, dottedAPI/language/fragment и старую версию, которая не закрывает facets даже после supported audit. Настоящий stdio regression подтверждает прохождение naturalquestion к model_unconfigured и отклонение явного DSL. Все обязательные интеграционные offline проверки привязаны к итоговому снимку в Router verification evidence.

Сохранены и ошибки процедуры: collision общего target между worktrees отброшена; одна фильтрация navigation_guard выбрала0tests и заменена exactnamed regression1passed. Статический reviewer выявил два случая article exception, оба исправлены до candidate inference. Ничего не скрывалось повторным живым запуском.

Следующие точки роста: дата/фаза выпуска и область применимости каждого утверждения, semantic completeness собственного witness, недостоверный `ok` при неполных условиях, ошибки генерации plan и точная классификация source_query_mismatch. Prompt guidance и title-version guard не решили всю задачу. Новые ошибки требуют отдельного frozen эксперимента, а не настройки на уже увиденном NEW4.

Результат разделён: реализация и offline checks есть; приёмка общего улучшения **не пройдена**; push/merge/deploy не выполнялись. Автозапуск Qwen, нагрузка, второй день и исходные20 не проверялись. Малый source-aware технический набор, один запуск и несбалансированный порядок не позволяют общей qualification или причинного вывода о приросте.

Сырой корпус сохранён локально в `feasibility/search-repair-corpus-2026-11-18.tar.gz`; [размер и SHA-256](../feasibility/artifact-manifest.json). Корпус, кэши, executable и credentials в Git не включены. Актуальный cumulative source ledger находится в results; OpenRouter429 отдельно запрещает повтор в этой кампании.
