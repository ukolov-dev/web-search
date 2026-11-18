# Официальный источник в полном поиске, 18 ноября 2026

Проверяется полный native research controller: настоящий planner → SERP → выбор URL → guarded HTTP/extraction → local Qwen → новый draft → прежний audit. URL, планы, тела документов и gold не передаются заранее. Это следующий отдельный эксперимент после oracle-проверки2/5→5/5, а не продолжение её метрики.

Серия **частичная, NOT_EVALUABLE**: остановлена на первой C01 baseline из-за `fetch_timeout` второго документа; кандидат и NEW4 не запускались. Парных сравнений нет.

Кандидат только меняет порядок реально найденных URL: для вопроса с токеном Python/CPython, Rust, PostgreSQL или SQLite отдаёт приоритет HTTPS-документации соответствующего проекта. Проверяются точный host, отсутствие credentials/nondefaultport; PostgreSQL ограничен `/docs/`. Нет генерации URL, доменных подсказок planner, замены вопроса или gold-зависимого выбора фрагмента. Остальные URL сохраняют прежний относительный порядок и site/DNS/IP/redirect guards. Приоритет официального домена не доказывает релевантность, версию или истинность текста.

Существующий общий API-documentation приоритет остаётся в baseline и candidate. Вмешательство в Session расположено под `cfg(test)`; по умолчанию false, production MCP не получает новый startup/tool аргумент. Пробы используют настоящий `Session.execute` и `Live` с общим test-only guard/pacing. MCP stdio в этом сравнении не вызывается; транспорт проверяется отдельно. Это проверка полного внутреннего запроса, не выпуск нового MCP-режима.

## Замороженный план

NEW4: Python lru_cache, Rust Vec::retain, PostgreSQL12 CTE, SQLite foreign_keys,9условий. OLD2: исходные T01/groupby и N04/CPython3.13,5условий, без изменения вопросов/языка/gold. Наборы считаются отдельно. Референсы gold проверил отдельный reviewer на4официальных страницах; это подготовительные чтения вне продукта, не найденные результаты research.

Implementation4e9ab74. До первого product/Qwen/generative вызова заморожены39файлов: Rust/driver/Cargo, gold, seed-ledger, отдельный testbinary,6fixtures только question/language. FreezeSHA256d114875209b4ccbd1a77fb3d5e0672b88199a28048bc6a3a68e1f733145ecd80, независимо сверены39/39.

Всего12проб, paired baseline/candidate с чередованием первого варианта: C01 baseline/candidate; P01 candidate/baseline; P02 baseline/candidate; P03 candidate/baseline; P04 baseline/candidate; C02 candidate/baseline. По одному повтору. Те же DeepSeek `deepseek/deepseek-v4.1-flash-1017`, localQwen, prompts/gates;180с/6генераций/4SERP/4documentreads/4Qwen на запрос, до72генераций на серию. Generative pacing10с учитывается в Session-бюджете;10с между пробами. CacheDB не подключён. Реальные остатки бюджета, clocks и SERP могут различаться; contexts не byte-identical.

Первая новая source/Qwen/model error, refusal или timeout останавливает серию без retry/resume. Прежние source blocks взяты из primary-draft-results и не очищались. Native guards сохраняются. Test-only общий `guarded` ставит pendinghalt до await: внешняя отмена сохраняет остановку, следующий future не опрашивается. Saved-generation guard также блокирует продолжение после неудачного format repair. Это более строгая дисциплина эксперимента, одинаковая для двух вариантов; generic production error-repair policy не изменялась.

## Фактический результат

Seznam вернул10результатов по настоящему planner query `itertools.groupby documentation consecutive keys sort`; Naver остался ранее отключённым. Уже baseline первым выбрал `https://docs.python.org/3/library/itertools.html`, получил HTTP200, извлёк полный body39110символов с SHAebd14031… и выполнил настоящий Qwen. Следующее чтение `https://www.pythonpool.com/pythons-itertools-groupby/` завершилось `fetch_timeout`. Общий экспериментальный guard остановил дальнейшую сеть и генерации. Сохранённый result имеет partial/model_error/fetch_timeout; это общий coarse stop_label после guard, а не отказ модели.

Session зарезервировал2generations, но POST реально был только1 — planner, с известной стоимостьюUSD0,0000425. Второй слот зарезервирован перед вызовом Guarded.generate; persistent halt отклонил его до опроса Saved.generate, поэтому draft POST не отправлен. Сохранён только model-01-plan. Это подтверждено независимо по callers/captures; поле cost_known=false у зарезервированного draft не доказывает неизвестный расход внешнего LLM. Qwen1, SERP1, logicaldocumentreads2 (одинHTTP200, у timeout нет наблюдаемого HTTPstatus). Эти логические счётчики не равны числу TCP/redirect hops.

Audit не запускался, итоговых claims0. Не запущены11trials:8NEWarms и3OLDarms; полных пар0. Ни прирост, ни потери, ни отсутствие новых ошибок кандидата не измерены. NOT_EVALUABLE не является FAILED semanticgate и не является PASS. Даже для C01 нет завершённого baseline-ответа.

Первый официальный документ был найден без oracle, однако преимущество кандидата из этого не следует: это baseline. Остановка после auxiliary-read timeout — намеренно строгий общий guard данной серии. Нельзя выдавать её за доказательство, что обычный production Session теряет уже прочитанный документ: штатная политика может продолжить работу после ошибки отдельного чтения и здесь не измерялась.

Native source ledger после пробы равен seed, все прежние блокировки сохранены. Native transport не добавил timeout как постоянный hostblock; отдельный campaign_exclusions в results фиксирует pythonpool.com и запрет повторять его в этой серии. Новый протокол, если понадобится, должен заранее различать transient document timeout и provider refusal, исключать неудачный host и сохранять найденные документы; текущая серия не возобновлялась и её stop rule не менялся.

## Проверки и ограничения

Pre-live: fmt,196unit+6stdio, clippy `-D warnings`, release build и driver self-check прошли;47unit/10stdio ignored. Первая сборка targeted-теста обнаружила private test-module visibility; исправлено до любой живой пробы. Независимый review PS01 обнаружил cancellation-hole в первоначальном wrapper; исправлен до freeze, адресный offline cancellation regression прошёл. Сеть в обычных тестах не вызывается.

Отдельно6stdio-тестов прошли со свежимreleasebinary.

ReleaseSHA25685e24099683f72885f668366bbbbdacba192cf89664e4a88c11b14b1725c5a5c отличается от предыдущей сборки; byte-identical binary не заявляется. Изменение выбора источника ограничено test-конфигурацией и не активировано в MCP.

Один прогон на4новых и2известных вопросах не является общей qualification, исходным20/day2 или доказательством повторяемости. Отсутствующие пробы не считаются семантическими потерями. Независимая агентская разметка — не человеческая приёмка. Push/merge/deploy и включение режима в MCP не выполнялись.

[Cases](../feasibility/primary-search-cases-2026-11-18.json), [results/latest native ledger и campaign exclusions](../feasibility/primary-search-results-2026-11-18.json), [независимая quality](../feasibility/primary-search-quality-2026-11-18.json), [driver](../scripts/primary-search-2026-11-18.py). Сырые SERP/HTML/document/Qwen/model/fixtures/freeze/reviews сохранены в `feasibility/primary-search-corpus-2026-11-18.tar.gz`; [размер/SHA](../feasibility/artifact-manifest.json). Binary, key и trustedsettings в архив не входят.
