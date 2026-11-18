# Отдельная source-aware серия, 18 ноября 2026

**Частичный результат, NOT_EVALUABLE:** восстановление после document timeout подтверждено, но сравнение остановилось на первой C01 baseline из-за длины audit reason201прилимите200. Кандидат и остальные11проб не запускались.

Новая серия полного native research controller использует прежние вопросы, gold, официальный-domain приоритет и модель. Изменена только общая для baseline/candidate политика документных ошибок. Прошлая серия остаётся остановленной; её результаты не заменяются и не объединяются с новой.

## Что изменено

Возвращённый `fetch_timeout`, page HTTP4xx/5xx, challenge/block и отказ `host_disabled` при чтении документа исключают все наблюдавшиеся исходные/redirect hosts и разрешают дальнейшую работу с уже прочитанными материалами. Исключения сохраняются между процессами. Прежние причины блокировок сохраняются; известный исключённый host отвергается до опроса native read future. `www.pythonpool.com` внесён в seed из прошлой campaign_exclusions до первого вызова, без повторного запроса.

При redirect на уже исключённый host `net::check_host` срабатывает до callback: этот конечный host уже есть в ledger, а наблюдавшаяся исходная цепочка также исключается. Отсутствие такого конечного host в observed_read_hosts не означает отсутствие блокировки. DNS/IP/URL/site/redirect guards не ослаблены. Неверный URL — fatal, не восстановимый timeout.

SERP/Qwen/model errors/refusal/non-stop finish и внешний controller timeout остаются fatal. Source recovery вызывается только внутри document-phase после успешной проверки отсутствия прежнего fatal halt. Pendinghalt до await сохраняется; отмена не разрешает следующий запрос. Успешный HTTP с обычной ошибкой JSON/schema может использовать прежний ремонт формата внутри общего бюджета, но отказ/ошибка транспорта модели не повторяются.

Эксперимент test-only: новый ignored entry вызывает общий `Session.execute`/`Live` с source-aware=true; старый entry сохраняет false. MCP startup/tool args и runtime-поведение не изменялись. Реальное сравнение проходит внутренний путь planner→SERP→HTTP/extraction→Qwen→draft→audit, не MCP stdio. Прямые URL/план/gold/тела документов модели заранее не передаются.

## Предварительная фиксация

Использован побайтово прежний casefileSHA2562263132f4c61df5f9a8cbc65f7181c1f024da1bcb29352ccc509011b0c735438: NEW4 Python/Rust/PostgreSQL12/SQLite,9условий, и OLD2 groupby/CPython3.13,5условий. NEW теперь обозначает историческую группу, а не свежую qualification; повторного reference browsing не было. Новая независимая gold-review подтверждает неизменность вопросов/языка/критериев.

Implementation26fdd95. До всех product/Qwen/LLM вызовов зафиксирован41файл: оба drivers, Rust/Cargo, gold,6question-only inputs, protocol, seed и отдельный testbinary. FreezeSHA25658c591c853b24a4e5871797b59941c5b1ad4b3fe12c812d4fc69db38dccb1899, независимо проверен41/41.

Порядок прежний12trials с чередованием arms: C01baseline/candidate; P01candidate/baseline; P02baseline/candidate; P03candidate/baseline; P04baseline/candidate; C02candidate/baseline. Один повтор. SameDeepSeek `deepseek/deepseek-v4.1-flash-1017` иlocalQwen, prompts/gates. По180с/6generation slots/4SERP/4logicaldocumentreads/4Qwen; максимум72slots на серию;10с между генерациями внутри Session deadline и10с между пробами. CacheDB не подключён.

Политика исключений общая, накопительная и не сбрасывается ради парности. Поэтому доступность источников у второго arm может отличаться; effects нельзя приписывать только сортировке без анализа конкретных чтений. Версия и релевантность официальной страницы по одному домену не гарантируются.

Новый `request-attempted-NN.json` записывается перед Model.research_call: это намерение транспортного вызова, а не доказательство dispatch при ошибке. Успешный response подтверждает реальный вызов; Session generation slots могут включать вызовы, отклонённые guard до такого marker. Эти счётчики не подменяют друг друга.

## Фактическое исполнение

C01 baseline: настоящий planner запросил `site:docs.python.org itertools.groupby documentation consecutive`, Seznam вернул10результатов, Naver остался отключённым. Первым найден и прочитан официальный itertools body39110символов, Qwen выполнил1отбор. Второе чтение `https://docs.python.org/el/3/glossary.html` получило `fetch_timeout`; это новый отказ другого URL, а не повтор прежнего pythonpool.

После ошибки источника выполнены настоящий draft и audit:3attempt markers/3responses/3generation slots. Таким образом документный timeout больше не обрывает работу с уже прочитанным источником. Известная стоимостьUSD0,000439992; неизвестного usage нет;429/providerrefusal/modeltransporterror0. Logicalreads2/SERP1/Qwen1; у timeout нет наблюдаемого HTTPstatus.

Draft содержит4claims с точными собственными цитатами, диагностически покрывает2/2условий C01. Raw audit вернул supported для всех4claims, missing=[], sufficient=true. Однако reasonA4 имеет201Unicodeсимвол при контракте≤200, и `apply_audit` отклонил весь audit (`audit reason bounds`). Finalclaims0, audit_completed=false; raw supported не засчитан как принятый ответ. Posthoc trimming/повторное обращение не выполнялись. Это локальная проверка формата после успешного LLM HTTP/JSON-ответа, не отказ провайдера и не новая ложность ответа.

11trials не запускались, полных пар0. NEW4/9условий: все8arms not_run; OLD2/5условий: только технически незавершённая C01baseline, остальные3arms not_run. Прирост/потери/новые ошибки кандидата не измерены. Raw draft2/2 — отдельная диагностика, не итоговая coverage и не сравнение arms.

Новый ledger сохранил все прежние отказы и pythonpool exclusion. Добавлен `docs.python.org: fetch_timeout` согласно замороженной консервативной whole-host политике. Это исключение тестовой кампании после таймаута одного URL, а не доказательство блокировки всего сайта. Такая гранулярность слишком широка для документации: дальнейшие Python-страницы были бы исключены. Ledger не очищался ради результата. В будущей отдельной работе нужно различать URL-level transient failure и host-level challenge; текущая политика после результата не менялась.

Ещё один отдельный followup — обработка слишком длинного свободного audit reason при неизменных verdicts/witnesses/guards:201символ вместо200не должен смешиваться с provider refusal. Сейчас bounds остаются прежними, новый repair не внедрён. Любое изменение и повторная оценка потребуют отдельного frozen snapshot; эта серия не возобновлялась.

## Проверки и границы

До запуска прошли fmt,199unit+6stdio, clippy-Dwarnings, release build, driver self-check;48unit/10stdio ignored. Дополнительно6stdio прошли со свежим release binary. Адресные offline checks проверяют document-only classification, recovery→следующая генерация, provider429→неопрошенный следующий future, сохранение старых block reasons, initial/redirect exclusions, исключённый host до native read и fatal URL validation без attempt marker. Нормальные тесты не обращаются к сети.

Первоначальная сборка обнаружила недобавленное test-only поле в существующем Live fixture; все конструкторы исправлены до freeze. Pre-live review SA01/SA02 обнаружил host_disabled classification и незаписанный halt при URL validation; обе ошибки исправлены до любых живых вызовов, review сохранён.

Release SHA2560d0f27daafdb0ed915d1c44173bade7b598c4f1f20c128c79ed7e1b3c272d542 отличается от предыдущей сборки; byte-identical binary не заявляется. Candidate/source-aware feature ограничены test-конфигурацией. Выпуска, push/merge/deploy и включения режима в MCP нет.

Качество и полнота серии фиксируются отдельно по фактическим ответам. Эта малая повторно используемая выборка не является original20/day2 или общей qualification; независимая агентская разметка не равна человеческой приёмке.

[Неизменные cases](../feasibility/primary-search-cases-2026-11-18.json), [results/latest campaign ledger](../feasibility/source-aware-search-results-2026-11-18.json), [независимая quality](../feasibility/source-aware-search-quality-2026-11-18.json), [новый driver](../scripts/source-aware-search-2026-11-18.py). Сырые source/model/Qwen/fixtures/freeze/reviews сохранены в `feasibility/source-aware-search-corpus-2026-11-18.tar.gz`; [размер/SHA](../feasibility/artifact-manifest.json). Binary/credentials/trustedsettings в архив не включены.
