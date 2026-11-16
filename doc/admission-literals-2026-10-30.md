# Document admission и literal-проверки v10 — 30.10.2026

**Четыре известных пропуска восстановлены, но критерий «без новых ложных direct» на новом наборе не пройден.** На24 новых запросах с локальным Qwen direct recall вырос27,78%→31,48% (TP15→17 из54), FP0→1; unknown direct1→1. V10 остаётся экспериментальным кандидатом и не получает положительной квалификации по этому результату. Default legacy сохранён.

## Что исправлено

Runtime — Rust, `request-evidence-v10-admission-literals`, opt-in `--evidence-memory`. Production diff ограничен `src/evidence.rs`; initial classifier, model profile/prompt, extraction и Qwen reranker не менялись. На запрос остаются3 generative calls,4 reads,4 Qwen calls и180с. Source JSON ограничен24000 символами на один model call; source_context_max — максимум, не сумма трёх вызовов. Default legacy сохранён.

| Старый пропуск | Причина v9 | Исправление и финальный old9 результат |
| --- | --- | --- |
| h01 C003: Python stdtypes | Reference попадал только на9-е место, namespace давал лишний URL overlap | Qualified API даёт leaf term; `_`/`-` identifier не разбивается; учитывается overlap той же карточки. Источник прочитан и принят audit |
| h04 C008: man7 dup | Safe SERP downgrade и неверный reference reserve оставляли источник вне10pool | Generic man-section URL допускается к reserve. Источник прочитан и принят audit |
| h03 C012: Object.freeze | Полный источник блокировал отсутствующий literal JavaScript | Unquoted JavaScript допускается через явный ECMAScript в том же body passage с subject и exact core witness. Audit принял источник |
| h04 C006: OpenBSD dup/dup2 | Body описывает close-on-exec flag без spelling FD_CLOEXEC | Только supported exact descriptor-flag quote, matching requirement и объявленный subject той же страницы открывают audit. Финальный old9 источник принят |

Reference reserve занимает максимум один из четырёх slots и выбирается среди подходящих `site:` карточек. Независимый review обнаружил offsite manual, который мог забрать слот до site-filter; его native red→green включён. URL shape даёт только приоритет чтения. Raw direct, отвергнутый grounding, не восстанавливается.

Quoted/backticked names, namespace, O_CLOEXEC/FD_CLOFORK, другие flags/headers остаются дословными. Перенос language cue из title или соседнего документа, перенос descriptor/status role и quote без real cited passage отвергаются. Literal exception открывает только audit; direct требует coverage всех условий, source scope/status, provenance, отсутствие unresolved conflict и отдельный accepted audit.

### Поправка по old9 до основного прогона

Первый v10 дал новый development FP p06C010: core quote был заголовком issue, повторённым в body. Теперь core witness, целиком содержащийся в actual page title H, становится ambiguous. SERP title T не используется: адаптер может вернуть там содержательный body teaser. Это structural guard против heading laundering, без domain/genre blacklist. Возможен консервативный FN, когда фактическая body-фраза одновременно является заголовком и модель выбирает именно её.

Один synthetic red→green и actual p06 coverage replay подтверждают: отклонён только core; anchors1..3 сохранены, T-only копия разрешена, без audit direct не возникает. Все ранние попытки сохранены.

## Известные случаи: development, не новый holdout

Общий old9: четыре старых запроса + пять известных FP controls,14 known-positive источников. Полный initial→reads→Qwen→coverage→audit. Источники для ранее выбранных документов сохранены; новые selected URLs вне narrow prefetch fail closed как cache_only_miss. Это диагностика, не источник основного результата24.

| Версия / попытка | TP | FN | FP | Direct recall | Unknown direct |
| --- | ---: | ---: | ---: | ---: | ---: |
| Текущая v9, новый запуск | 4 | 10 | 1 | 28,6% | 0 |
| v10 до heading guard | 5 | 9 | 2 | 35,7% | 0 |
| Финальная v10 | 7 | 7 | 1 | 50,0% | 0 |

Все четыре целевых документа в финальной v10 получили direct; на четырёх исходных запросах текущая v9 дала TP1/6, финальная v10 —5/6. Историческая предыдущая v9 имела2/6; её не подменяем текущим baseline. Один прежний positive h01C012 не был прочитан при новом initial output; его потерю не вычитаем из оценки. Новых FP IDs относительно текущей v9 нет. Остаётся общий semantic FP p11C004: excerpt называет MATERIALIZED/NOT MATERIALIZED и общие правила CTE, но не устанавливает необходимое направление каждого override.

Повтор v9 тоже вернул этот FP, хотя исторический предыдущий запуск имел FP0. Для Docker p06 initial и coverage inputs обеих версий были одинаковы, их модельные coverage outputs различались. Это наблюдение нестабильности судьи; разница полного прогона не является чистой причинной оценкой селектора. Первая проверка h04C006 также оставляла FD_CLOEXEC gap из-за перестановки check→anchor; финальный запуск дал корректную property quote. Guard не ослаблялся ради этой вариативности.

Данные: [old9 comparison](../feasibility/admission-literals-old9-2026-10-30.json), [scoped red→green](../outputs/admission-literals-2026-10-30/bug-reproducer-report.md).

## Новый24: протокол

24 новых operation relations,6 false-premise controls. Точные query/forms не менялись после фиксации; проверка пересечений22 прежних case files не нашла совпадений operation/query. Это собственный инженерный набор английских технических запросов, не случайная выборка всего веба и не стандартный независимый benchmark.

[Cases](../feasibility/admission-literals-cases-v1.json), [заранее заданные material criteria](../feasibility/admission-literals-plan-v1.json), [frozen source gold](../feasibility/admission-literals-gold-v1.json).

Seznam/Naver сделали48 исходных поисковых обращений без source errors:440 карточек,424 unique URLs. До обеих модельных ветвей завершено сохранение всех URL через Rust public-URL/DNS/IP/redirect guards:395 fetch attempts,2 same-URL snapshots импортированы из прежних датированных runs,27 URL получили failure envelope по уже заблокированному host. Всего298 unique raw HTML responses, включая challenge;126 failure snapshots. Из raw HTML ошибка/неподходящий HTTP status не превращается в negative grade. После пре-выборки заблокированы25 hosts; mask одинакова для обеих ветвей. CAPTCHA/403/429 не обходились, повторов тех же URL в этой пре-выборке не было.

Автор мог начать отдельный кейс после сохранения всех его URL; глобальная raw freeze сделана после завершения всех424, до final gold и model runs. Полные canonical originals дали54 grade2,158 partial,76 unrelated,152 unknown. Partial считается не-direct; unknown не считается negative. Три source-only annotators и ротационная peer-проверка подтвердили все288 известных оценок и456 точных цитат с UTF-8 ranges. Это review участников текущей команды, не независимая приёмка продукта. Scope warnings об optional/внутренне противоречивых утверждениях сохранены; никакой факт не добавлялся из памяти по репутации URL.

Gold заморожен **2026-10-30T19:08:06.063473+00:00**, до первого NEW24 inference. Initial code freeze был до начала source annotation; затем отдельный old9 выявил heading loophole. Финальная code freeze после его поправки: **2026-10-30T18:55:03.919645+00:00**. Source annotation уже шла, но criteria/grades под runtime guard не менялись. Эта хронология раскрыта; новых предсказаний для донастройки не было. Одна проверка helper не исчерпывает запрет раннего запуска: её filename pattern уже, чем реальные call artifacts. Перед первым cargo отдельно проверено отсутствие обеих arm directories; существующие manifest/call timestamps подтверждают фактический порядок. Source/Cargo/fixtures заморожены; binary hashes записаны в execution manifests и проверены по реальным executable, отдельной preregistered auth/config snapshot нет. Содержимое авторизации не архивировалось.

Обе ветви используют одинаковые frozen SERP и raw bytes, cache-only и реальные initial calls. Extraction и Qwen имеют общий неизменный код; каждая ветвь сама отбирает документы/свидетельства и извлекает выбранные URL. Нет oracle URLs, initial bypass или gold labels в model input. Baseline — актуальная v9, fixed — v10. В обеих DeepSeek V4.1 Flash1017: initial none/4096, coverage none/6000, audit low/6000 с excluded reasoning text; Qwen3-Reranker-0.6B-4bit локально. Запуски последовательные, без повторных генераций для выбора удачной оценки.

## Основной результат24

| Метрика | Текущая v9 | Финальная v10 |
| --- | ---: | ---: |
| Direct recall по54 known-positive | 27,78% | 31,48% |
| TP / FN | 15 / 39 | 17 / 37 |
| Known false direct | 0 | 1 |
| Unknown direct | 1 | 1 |
| Precision на размеченных direct | 100% | 94,44% |
| Positive в10pool | 48 / 54 | 49 / 54 |
| Positive действительно прочитаны/извлечены | 27 / 54 | 28 / 54 |
| Read recall | 50,00% | 51,85% |
| Positive среди всех returned top10 classes | 49 / 54 | 49 / 54 |
| Initial valid / length / stop-empty | 24 / 0 / 0 | 24 / 0 / 0 |
| Локальные Qwen calls / pairs | 65 / 1399 | 69 / 1600 |
| Qwen fallback | 0 | 0 |

Новый false-direct ID ровно **l19/C004**: [gist о создании orphan branch](https://gist.github.com/seanbuscay/5877413). Поздняя correction ALL→ALL tracked и исключение untracked уже присутствуют в одинаковых выбранных B1/B2/B3 обеих ветвей. V10 выбрала раннюю чрезмерную фразу для core и audit её принял; source не устанавливает все требуемые switch-created unborn/no-initial-commit и index условия, а checkout-пример не переносится на switch. Это семантическая ошибка выбора/оценки свидетельства, а не отсутствие позднего фрагмента. Frozen grade1 сохранён; URL/жанр/дата не использованы как blacklist.

Наблюдаемые gains: l11C012, l12C009, l16C009; loss: l22C011 при уже прочитанном source. Net TP+2, +3,70п.п. Query bootstrap95% для delta **−3,23…+10,91п.п.**, включает0. Это не доказательство устойчивого улучшения. Initial request целиком совпал в24/24, но модельные outputs не детерминированы; выборка одна, Qwen одинаков в обеих ветвях.

Unknown direct у обеих — l17C014 Presto: full source fetch_timeout, audit принял SERP. Полный original не наблюдался, поэтому это не TP и не FP; полная precision не доказана.

Из37 оставшихся FN:20 не выбраны в четыре чтения,5 вне10pool,6 имеют неподдержанный coverage,3 отвергнуты audit,1 имеет invalid cited quote,1 read error,1 причина недостаточно определена. Это ранние наблюдаемые gates, не обещание причинного эффекта их обхода.

Ручная проверка FP и пяти FN обнаружила разные проблемы: l02 bisect/l07 Optional — нужная другая ветка поведения есть в extraction, но не в выбранных passages; l10 maps.Clone/l24 CompareExchange — нужные clauses уже переданы, coverage всё равно отказывает; l20 committer-default section потерян extractor, а audit дополнительно путает направление author/committer requirement. Ни одно из этих наблюдений не использовано для донастройки после NEW24 inference.

Один основной подсчёт и независимый recount совпали:24/24 cases обеих ветвей,424 raw URLs,456 source-gold quotes,140 read-provenance records,171 published direct-proof ranges. После обеих ветвей code/gold/raw hashes не изменились. Peer PASS означает корректность счёта/целостности; **quality gate остаётся FAILED**.

Данные: [полные metrics/case deltas](../feasibility/admission-literals-results-2026-10-30.json), [source/results corpus](../feasibility/admission-literals-corpus-2026-10-30.json), [все FN/FP gates](../feasibility/admission-literals-misses-2026-10-30.json), [ручной source-boundary review](../feasibility/admission-literals-source-review-2026-10-30.json), [execution/peer evidence](../feasibility/admission-literals-execution-2026-10-30.json).


Метрики считаются по54 known-positive retrieved источникам, не по всему вебу. Unknown direct показаны отдельно. По всей выборке учитываются все24 запроса, включая отсутствие полного источника и ошибки. Timing replay на cache/provider warm state не является изолированным SLA или сравнением производительности Rust.

## Проверки и границы

Финальные84 обычных теста:79 unit +5 stdio; fmt, Clippy all targets, release build и5 release stdio прошли. Реальный свежий release MCP после поправки: status ok/errors[],3 generative/4 reads/4 local Qwen,61 pairs. Это проверка настоящего продукта, отдельная от cache-only benchmark.

Original20 SHA b67e3269830ae0e976aa9b4c5dc2f6b9c3e922ad1adda35738289d6b1b8325d6 и B17 не менялись. URL/DNS/IP/redirect/site guards, startup trust и default legacy сохранены. Новых dependencies, CLI/MCP parameters, VPS/deploy/global config/hooks нет. Git не инициализирован; используются реальные snapshots/diffs/hashes, не утверждение о чистом Git diff.

Основной A/B:130 priced generative calls, **$0,024671032** по provider usage; все вызовы обслужил Morph с тем же pinned model. Qwen134 calls/2999 pairs,0 fallback. Вся задача с тремя old9 attempts:206 priced calls/**$0,041967110**, ещё6 unpriced generations двух real MCP smoke; их стоимость не выдумывается. Source pre-acquisition заняла1593,02с отдельно от per-query replay. Median replay17,10→16,59с при суммарном399,08→506,16с не даёт оснований заявлять ускорение: cache/provider state и форма вызовов различаются.

Результат не даёт основания объявить v10 безопасной заменой v9. Следующий узкий предмет работы — привязка цитаты к операции и полным условиям, включая поздние corrections, а не ещё один reranker. Это предложение по результатам теста; frozen NEW24 не использовался для нового runtime tuning.


Нулевой observed FP не доказывает общий zero-FP. Один общий old9 semantic FP и вариативность model condition binding остаются известными ограничениями. Новый24 не донастраивался после inference; проверка MiMo+v10, нагрузки, VPS и day2/квалификация исходного pilot20 не выполнялись. Формальная независимая ready_to_merge acceptance не проводилась.

Контекст проекта обновлён: карта источников, README, MCP-дополнение, текущая project-запись и finding revision20. Router lint0errors/0warnings, index свежий, context-check passed; doctor15OK/5WARN/0FAIL: непроверяемые Git ignore/tracking, source_globs coverage и отсутствие ready backlog; это не независимая приёмка.
