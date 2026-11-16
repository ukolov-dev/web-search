# Ложные direct и потери coverage: v6 и новый полный A/B

30.10.2026, `TASK-20261030-181158-001`. Продолжение [v5 extraction/selection](extraction-selection-2026-10-30.md). Это собственная инженерная проверка со слепой разметкой источников, не независимая приёмка пилота.

**На новом8/154 direct recall вырос25,0% → 31,25%: TP4→5,FP0→0,FN12→11.** Unknown direct0 у обеих веток. Отдельный повтор двух прежних false direct дал **2→0**. На новом наборе есть небольшой прирост именно direct, но всего пять верных direct и descriptive95% interval включает отсутствие выигрыша. Общую надёжность и полноту поиска этот прогон не доказывает.

## Что исправлено

### Контекст не должен исчезать из-за выбора цитаты

Coverage получала три выбранных Qwen body windows и title/snippet, но `audit_context` передавал дальше только процитированные окна. Так пропадал B3 со статусом `Resolution: unsatisfied` у CSS design review. `literal_gaps` аналогично не видел ORDER/CSS, хотя они уже были в переданном контексте этого же источника. Это ошибочно исключало источник до audit.

В `request-evidence-v6-context-audit` literal presence и audit используют **все уже отобранные bounded passages своего кандидата**, включая непрoцитированные body/header/title/snippet. Полный100k body или URL не подставляется вместо свидетельств. R0 остаётся исходным вопросом; supported checks по-прежнему требуют короткую дословную цитату и реальные ID. Семантическая проверка всех фактически запрошенных условий и accepted audit обязательны. Наличие слова в соседнем API не разрешает direct.

В audit не дублируются короткие quotes рядом с уже переданными окнами: передаются ссылки checks→passages и общий список requirements. Это сохраняет context cap24000 без потери непрoцитированных supplied passages. При превышении cap или ошибке нет принятия старых direct.

### Источник со статусом proposal не становится текущей документацией

Одного изменения prompt оказалось недостаточно: первый повтор exact old coverage снова принял CSS issue. Поэтому Rust дополнительно проверяет явные publisher warnings в snippet/начальном body: `Resolution: unsatisfied` и `This documentation is for an unsupported version`. Для обычного текущего запроса они запрещают coverage eligibility и direct, даже если модель ответила supported.

Это не запрет жанра issue или старой даты: слово Closed само по себе не отклоняет исправленный issue. Для явно исторического запроса/самого design review предусмотрено исключение; для unsupported docs возможна явно запрошенная числовая версия, присутствующая в настоящем page title. Audit всё равно проверяет соответствие вопросу. Распознавание отдельных warning phrases консервативно и не универсально для всех сайтов/языков.

Дополнительный source review поймал ошибку исключения: название **History API** могло ошибочно отключать guard по слову history. Исправлено до первого fresh v6 вызова: теперь нужны отдельное historical/историческ-слово или явная двухсловная фраза, а не generic history/draft/proposal. Regression101→0. Записи версий вроде `v7.0`/`7.0,` не нормализуются для этого исключения: возможен лишний отказ, лучше явный числовой токен7.0.

### Явный конфликт требует отказа от бесспорного direct

Новый третий вызов `audit_evidence` возвращает labels и до4 conflict pairs. Для каждой стороны нужны ID кандидата, ID passage и точная цитата16..240. Rust атомарно проверяет все ID, уникальность пар и существование цитат в supplied snippet/body. Неизвестная ссылка или выдуманная цитата отклоняет batch. Валидный неразрешённый конфликт принудительно понижает **обе стороны**, даже если labels говорят supported=true; причина и quotes видны в `audit_conflicts`.

В старом Git-примере корейская статья утверждала потерю stash при конфликте, официальный документ говорил обратное. Нельзя выбрать победителя только по домену или памяти модели. При таком конфликте правильный источник тоже может временно стать background. Это сознательный компромисс; он не означает, что обе стороны фактически неверны.

Audit сравнивает одну материальную связь в одинаковых условиях, сохраняет отрицание и асимметрию сравнения API. Разные свойства разных API не считаются противоречием. Проверяются только audit targets; контрпример из непрочитанного или не прошедшего coverage источника пока может остаться вне сравнения. Наличие точных quotes доказывает происхождение, но не математически доказывает семантику конфликта. Reviewer и critic остаются одной моделью.

## Проверка прежних случаев и неудачные этапы

Сохранены исходные v5 passages, hashes, offsets, initial labels и успешная старая coverage. Test-only replay восстановил их без нового поиска/чтения/перескоринга и вызвал настоящий новый audit. Это отдельный компонентный контроль, не основной benchmark с обходом initial.

Первый v6 replay: Git false direct отклонён, CSS false direct остался. После status guard повтор тем же named command отклонил **оба**. Git conflict pair с точными source quotes действительно получен и проверен; обе стороны получили quarantine. CSS неподтверждённый design review исключается guard до принятия direct. Финальная проверка «старый false ID не direct» не ослаблялась. [Native red→green report](../outputs/direct-coverage-2026-10-30/bug-reproducer-report.md).

Пять старых вопросов также прогнаны целиком как development. Там обнаружилось лишнее повышение unsupported PostgreSQL7.0 документа; добавлено явное правило для этого предупреждения. Полный первый development не объявляется результатом финального кода. Все14 генераций и ошибочный первый audit сохранены и включены в цену. Случаи Rust read_until delimiter и SQLite recursive_triggers не «починены» ослаблением API matcher: недостающие отношения/точный identifier остаются причиной отказа.

## Протокол нового полного A/B

Baseline — текущая v5; fixed — v6. Обе ветки используют `deepseek/deepseek-v4.1-flash-1017` через OpenRouter и локальный **Qwen3-Reranker-0.6B-4bit / oMLX0.7.0**. Initial profile одинаков: reasoning none/max4096; coverage6000. Новый evidence audit3200 вместо legacy2400, без дополнительного вызова. Legacy audit_pages/initial prompt сохранены. Общие пределы: **3 генерации / 4 чтения / до4 local reranks / 180с**, context≤24000. Default legacy и доверенный startup сохранены, новых MCP-аргументов нет.

Новые8 запросов и retrieval зафиксированы до inference. Реально получены16 Seznam/Naver200,154 карточки. Одинаковые source pool и raw acquisition snapshots: HTML/status/final URL/hops/hash либо ошибка; каждая ветка извлекает и выбирает документы обычным путём. Initial не обходился, Qwen реально использован в обеих ветках. URL/DNS/IP/redirect/site safeguards, original20 и B17 не менялись; блокировки не обходились.

Первый baseline получил OpenRouter429 до валидных labels и остановился. Read-only проверка подтвердила действующий ключ и не исчерпанный per-key cap; точная причина429 не установлена. После129с без генераций начат один новый run с тем же ключом/моделью/маршрутом; он завершился. Автоматических runtime retries нет; отказ сохранён отдельно и не включён в качество. Различие credit/rate limits описано в [официальной документации OpenRouter](https://openrouter.ai/docs/api_reference/limits); персональные параметры ключа в durable records не сохранялись.

До запуска fixed, после baseline, source review обнаружил HistoryAPI edge case. Новые predicted labels не просматривались; исправление не основано на запросах/метриках этого набора. Старые freeze/source сохранены; amendment и финальный hash зафиксированы **до первого fresh fixed вызова**. Это отступление от первоначального code freeze раскрыто, а не скрыто как неизменный исходник. После старта fixed код не менялся.

Gold размечен отдельным агентом по source cards/full retained HTML без просмотра новых outputs/labels. Три заранее выбранных primary/secondary страницы прочитаны только для gold и не переданы моделям. Из154 карточек65known:16positive,49partial/unrelated;89unknown. Gold frozen после inference, до раскрытия predictions. Это один автор с частичным покрытием, не независимая acceptance. Старые performance/proposal sections не считаются current только из-за тематического совпадения; конкретное правильное описание существующего поведения в issue может быть positive. Все positive source quotes/HTML hashes/UTF-8 ranges проверены.

## Результаты

| Метрика | V5 + Qwen | V6 + Qwen |
| --- | ---: | ---: |
| Initial valid / length / empty-stop | 8/8 / 0 / 0 | 8/8 / 0 / 0 |
| Initial reasoning tokens | 0 | 0 |
| True direct / false direct / FN | 4 / 0 / 12 | **5 / 0 / 11** |
| Unjudged direct | 0 | 0 |
| **Direct recall** | **25,0%** | **31,25%** |
| Precision среди known direct | 4/4 | 5/5 |
| Вопросы хотя бы с одним known true direct | 2/8 | **3/8** |
| Positive в coverage pool | 14/16 | 15/16 |
| Positive реально прочитаны/извлечены | 12/16 | 12/16 |
| Returned top10 positive, любой класс | 14/16 | 15/16 |
| Генерации / чтения | 20 / 32 | 20 / 32 |
| Qwen calls / pairs / fallback | 28 / 223 / 0 | 28 / 248 / 0 |
| Проверенные direct proof ranges | 20 | 25 |
| Median full replay | 35,810с | 24,742с |
| Reported cost | $0,003793002 | $0,003771140 |

Все40 генераций обслужил Morph. На новом8 conflict pairs не потребовались; их реальная проверка показана отдельным старым Git-контролем. Новый FP не появился. Но FP0→0 на9 суммарных direct не является универсальной гарантией. Direct recall +6,25п.п.; descriptive query-bootstrap95% **−12,5…+25,0п.п.**, включая0. Это наблюдаемый небольшой прирост, не доказательство устойчивого выигрыша на большом корпусе.

Timing последовательного replay включает переменную скорость провайдера и разные raw cache hits0→28; после429 не делается вывод об ускорении или SLA. Метрика recall — по16 известным релевантным **retrieved sources**, не по всем страницам веба, не answer accuracy; несколько source URLs могут описывать одно отношение.

| Вопрос | Positive | TP v5→v6 | FP v5→v6 |
| --- | ---: | ---: | ---: |
| Git cherry-pick --no-commit / initial index | 1 | 0→0 | 0→0 |
| Python dataclasses.asdict / recursive deepcopy | 1 | 0→0 | 0→0 |
| Rust Rc/Weak / value versus allocation | 3 | **0→1** | 0→0 |
| PostgreSQL NOT IN/NOT EXISTS / NULL | 3 | **1→2** | 0→0 |
| SQLite ADD COLUMN / NOT NULL/default | 1 | 0→0 | 0→0 |
| Docker ARG/ENV / multi-stage scope | 1 | 0→0 | 0→0 |
| CSS visual/focus order/accessibility | 5 | **3→2** | 0→0 |
| HTTP421 / non-idempotent retry | 1 | 0→0 | 0→0 |

При одинаковом числе реально прочитанных positive появились два direct по Rust/PostgreSQL, потерялся один по CSS. Net+1 относится к финальной классификации, а не просто top10. Full pipeline генерирует ответы заново; source/ordering/anchor вариативность остаётся фактором, поэтому весь net gain нельзя приписать одному переключателю без дальнейшей абляции. По новому набору код больше не дорабатывался.

## Что осталось

- Четыре positive не были прочитаны: Git primary, PostgreSQL primary, один CSS источник и HTTP421 secondary. Часть есть в coverage pool, но краткой карточки недостаточно. Улучшение audit не заменяет document selection.
- CPython источник прочитан, но coverage не подтвердила checks; вклад extraction, выбора окон и ошибки judge ещё требует разделения. Rust mirrors и SQLite сохранили gaps по отдельным условиям. Docker windows закрывают ARG, но не ENV inheritance/persistence. Это следующие задачи получения/отбора evidence, не основание принимать missing как supported.
- CSS потерял один direct из-за непокрытого выбранного anchor. Избыточные/нестабильные anchors и выбранные fragments ещё ограничивают recall.
- HTTP keyword `non-idempotent` трактуется literal matcher как составной identifier; эквивалентная отрицательная формулировка не проходит. Глобальная замена API literals на синонимы не включена из-за риска новых FP.
- Конфликты вне audit targets и статусы без распознаваемой явной маркировки могут быть пропущены. Quarantine не устанавливает, кто прав, и способен понизить корректный источник.

## Проверки, стоимость, артефакты

Прошли **63 обычных теста (58unit+5stdio)**, fmt, all-targets clippy `-D warnings`, release build и5 stdio проверок свежего release. Реальный release MCP без raw cache прошёл contract smoke: initial valid,3генерации/4чтения/**4Qwen/61pairs**. Ответ `partial`: один начальный ungrounded direct правильно понижен (`unsupported_direct_evidence_or_constraints: 1 downgraded`); это не error-free quality-пример и не девятый вопрос A/B.

Всего по задаче58 priced successful generations, **$0,010278142 reported cost**, включая development и неудачный первый audit; ещё3 MCP generations без выгруженного usage и1 refused429 attempt без price response. Их стоимость неизвестна. Предварительные/финальные source snapshots, scope review, code-freeze amendment и все реальные логи сохранены; старые ошибки не удалялись.

- [Новые8](../feasibility/direct-coverage-cases-v1.json), [frozen gold](../feasibility/direct-coverage-gold-v1.json), [результаты/per-case](../feasibility/direct-coverage-results-2026-10-30.json).
- [Durable corpus](../feasibility/direct-coverage-corpus-2026-10-30.json): источники/raw HTML, extracted documents каждой ветки, model inputs/outputs, manifests/hashes и gold.
- [Полный учёт стадий, отказа и amendment](../feasibility/direct-coverage-execution-2026-10-30.json), [native bug report](../outputs/direct-coverage-2026-10-30/bug-reproducer-report.md).
- [Gold builder](../scripts/direct-coverage-gold-2026-10-30.py) запрещает перезапись; [offline analyzer](../scripts/analyze-direct-coverage-2026-10-30.py): `python3 scripts/analyze-direct-coverage-2026-10-30.py`.

Context lint/index freshness/smoke прошли; doctor15ok/5warn/0fail (Git/tracking, tooling coverage и ready backlog). Verification evidence и last_result сохранены в Router. Формальное completion partial: независимой ready_to_merge приёмки нет, собственные проверки её не заменяют.

Логи и code snapshots — `runs/direct-coverage-2026-10-30`, временная evidence, не долговечная память. Original20 SHA-256: `b67e3269830ae0e976aa9b4c5dc2f6b9c3e922ad1adda35738289d6b1b8325d6`. Без Git refactor heuristic не получил diff; собственный scoped review и успешные тесты не заменяют независимую приёмку. MiMo+v6 quality, VPS, нагрузка, day2 и общая qualification не выполнены.
