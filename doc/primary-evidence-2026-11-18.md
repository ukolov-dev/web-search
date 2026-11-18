# Первичное контрдоказательство при неизменном audit

**Улучшения в завершённом повторе нет.** Official reference body был доступен в rejection context, но baseline и candidate сохранили одинаковые2false и2unknown, при сохранении4/4 положительных claims. Выполнено5/12попыток: после четырёх ответов начало второго повтора получило HTTP429. Повторов после отказа нет; остальные7calls не запускались. Итог **INCOMPLETE_NO_OBSERVED_IMPROVEMENT**, без приёмки продукта.

## Гипотеза и метод

После неудачных изменений audit проверен другой фактор — **материал для проверки**, а не prompt/schema. Восстановлены два настоящих full audit batches по4claims: T01/groupby из regression18.11 и N04/CPython3.13 из candidate18.11. Ранее для RustOption выяснено, что eager/lazy оговорка уже присутствовала в выбранном фрагменте: отсутствие этой фразы не объясняет тот miss, поэтому перестановка Rust-фрагментов здесь не испытывалась.

[Cases/gold](../feasibility/primary-evidence-cases-2026-11-18.json), [фактические результаты/usage/ledger](../feasibility/primary-evidence-results-2026-11-18.json), [независимые оценки и source review](../feasibility/primary-evidence-quality-2026-11-18.json).

Baseline получает восстановленные исходные claims, ownquotes, facets, sources и rejection chunks. Candidate добавляет только S3 — известный первоисточник с максимум3фрагментами по1200chars. Исходный claim или его собственная цитата не заменяются, не расширяются; S3 — **только для отклонения/контекста**, не для заполнения недостающей опоры собственного witness.

Оба варианта используют тот же generic RULES/AUDIT_PROMPT и Audit schema, прежнюю `deepseek/deepseek-v4.1-flash-1017`. Scoped override из прошлого эксперимента не используется. `scope_saved_audit_experiment` вызывается с ARM=baseline для **обоих** material variants. Единственная содержательная разница model contexts — sources/rejection_context; budgets/claims/quotes/facets/language совпадают. Это проверено независимо и по фактическим requests.

План до первого generative вызова:3pairedreplicates на каждый из2batches, всего12calls, по90с/3072tokens, pacing3с, порядок variants чередуется. Три повтора запланированы заранее, не являются retry неудачного результата. Первая ошибка/отказ останавливает всю серию; никакой настройки prompt после ответов.

## Откуда взят первичный материал

Через **наш native MCP extract** однократно прочитаны:

- `https://docs.python.org/3/library/itertools.html`:39110chars, полная extraction; groupby consecutivekeys/keychanges и generally/usuallysorted попали в S3.
- `https://docs.python.org/3.13/whatsnew/3.13.html`:100000chars, extraction truncated. Нужные experimental/notdefault/separateexecutable и officialWindows/macOSinstallers находятся до границы и попали в S3. Полнота всей страницы не заявляется.

Оба ответа200 проверены не только по статусу: независимый content review подтвердил фактические API-параграфы, origin и hash; шесть выбранных S3-slices точны. Сохранены HTML, native tool wire, extraction metadata, original/final URLs и ledger. Источники не перечитывались после отказов; все прежние engine/host blocks сохранены, новых отказов источников нет.

По одному **настоящему localQwen** вызову на original question+saved primary text; стандартный текущий `reranker::select`, без gold, ручного выбора фразы, новой модели или fallback. UTF8byte offsets переведены в Unicodecodepoints относительно exact extracted body; version S3 — явно test identity `primary-saved-extract-v1` с URL/bodyhash, а не доказательство live cache version.

Это **known-URL oracle-source component**, не поиск нужной страницы. Прежние4HTTPattempts старого research здесь не повторялись. Интеграция «добавить после них ещё один read» нарушила бы runtime budget: потребовалось бы заранее заменить неудачный/нерелевантный read первичным источником. Такой controller/retrieval выбор и общий бюджет исходного MCP-запроса этим тестом не доказаны и не изменены.

## Результат завершённого r1

| Variant | Сохранённые positive | False kept | Unknown kept | Удалено прежних false |
| --- | --- | --- | --- | --- |
| baseline | 4/4 | 2 | 2 | — |
| primary context | 4/4 | 2 | 2 | 0 |

В обеих variants остались P01/A4 — необоснованное универсальное требование сортировки, и P02/A1 — обязательная собственная сборка/отсутствие officialdistribution. P02/A2/A3 остались unknown из-за области macOSinstaller/framework и недостаточности собственной цитаты. Raw supported и after-apply kept совпали: все8claims в каждом варианте.

Новых FP, unknown и потерь positive в r1 не наблюдалось. Но улучшения нет, а три повтора не завершены: нельзя объявлять успешную приёмку или устойчивую эквивалентность. P01/A2 в gold рассматривается как streaming/no-global-lookahead утверждение; не как гарантия отсутствия внутреннего чтения следующего элемента. Эта интерпретационная граница зафиксирована до inference.

Querycoverage завершённой части4/5 у обоих вариантов. Quality также сохраняет frozen denominator всех3повторов:4/15; это неполная серия, не recall поиска. Отсутствующие вызовы не считаются смысловыми отказами или потерями положительных ответов.

`r2-P01-primary` получил настоящий OpenRouter HTTP429. У этого вызова нет usage; его стоимость неизвестна. Известная стоимость четырёх завершённых generative calls — **USD0,000773024**. Третий повтор и оставшаяся часть второго не запускались. Никакого обхода лимита, смены провайдера/ключа/модели или повторения после отказа не было.

В этих двух батчах отсутствие релевантного counterevidence не объясняет сохранённые ошибки: оно было передано. Это не доказательство общей причинности или внутреннего механизма модели. Исследована именно добавка rejection material к замороженным claims, а не новая генерация ответа по primary source.

## Проверки, снимки и границы

[Test-only Qwen entry](../src/research_scope_audit_experiment.rs), [строгий restore](../scripts/primary-evidence-fixture.py), [runner](../scripts/primary-evidence-2026-11-18.py). Implementation snapshot05431b2, база4f60e99; production runtime и общий audit не менялись. Проверены original document SHA/version/title/publication, unique chunk/ownquote slices и восстановленные глобальные offsets. Неоднозначность останавливает restore. Ответы валидируются existing finish/refusal/contract guards, не меняются постфактум.

Gold и исходные вопросы заморожены до local selection. Qwen helper был закоммичен до локальных вызовов; полный driver/source/settings freeze записан **после2localQwen, доgenerative**. Это ограничение предварительной фиксации локальной стадии, оно не скрывается. Корпус после просмотра generative не подгонялся. Startup-settings file не редактировался.

Старый exact context_text сохранён в raw captures. Restore Session и новый prepare нормализуют standalone budget/clock обоих variants к одному frozen значению; это не byte-identical повтор старого живого audit18.11. Loader context.chunks становится фактическим serialized rejection_context; неиспользуемый старый context.rejection_context внутри loader fixture не подменяет payload. Фактический model input сверялся с context_text каждого варианта.

Placeholder Session counters/modelcalls0 и пустой source.text_hash — особенность старого saved Session loader. Реальные source2/Qwen2/generative5attempts учтены отдельно по wire, trace, requests и runner ledger; placeholder не выдаётся за production budget/cache/live evidence. Прежний source refusal state не сбрасывался; это отдельная явно разрешённая серия, остановленная при новом HTTP429.

Независимая агентская разметка и code review сохранены; это не человеческая приёмка. Обязательные offline Rust/fixture/driver/Router checks привязаны к итоговому снимку в verification evidence. Первоначальный collector имел исправленный SyntaxError `else0`; он не создавал вызовов и не менял уже сохранённые ответы.

Эксперимент частичный. Release/runtime не активированы, новых API/зависимостей нет, push/merge/deploy не выполнялись. Исходные20/day2, полный поиск, реальная интеграция primary-first чтения и устойчивая повторяемость не квалифицированы.

Сырые HTML/inputs/modelresponses/Qwen/freeze/reviews: `feasibility/primary-evidence-corpus-2026-11-18.tar.gz`, [размер/SHA](../feasibility/artifact-manifest.json). Бинарники, credentials и raw bodies в Git не включены. Latest source ledger находится в results.source_state_after; provider stop — в summary/stop evidence.
