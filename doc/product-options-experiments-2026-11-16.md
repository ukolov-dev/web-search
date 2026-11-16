# Эксперименты улучшения research, 16 ноября 2026

Проверяются все три направления [обзора](product-improvement-research-2026-11-16.md). Пользователь разрешил эксперименты; это не разрешение объявления любого результата улучшением. Task TASK-20261116-110523-001, run AGENT-RUN-20261116-110639-001. Существующая рабочая ветка сохранена, исполнители работали в отдельных worktrees.

## Компоненты

- [Section selection](section-experiment-2026-11-16.md): в сохранённом F02 полный prefix100000 теряет нужную норму;1660символов выбранного раздела её сохраняют. Прототип только test-only. Независимый reviewer не рекомендует product wiring: одна секция может потерять обязательное исключение из другой. Синтетический второй RFC не доказывает переносимость на реальные источники.
- [Normalized inference](inference-obligations-experiment-2026-11-16.md): baseline8/10 и candidate8/10, но candidate восстановил2positives ценой2новыхfalse accepts. Это FAILED. Новый transfer4/4 не отменяет провал; никакого runtime guard/whitelist не перенесено. Baseline позже candidate, без рандомизации; test-only component сравнение не равно полному audit.
- [Missing followup](research-missing-followup-2026-11-16.md): реализован узкий repair повторной формы/закрытой facet. Новая корректная форма сохраняется, сайт/версия проверяются прежними guards, бюджеты прежние. Fake red→green подтверждает ветвь, не live utility. Этот кандидат единственный участвует в live paired сравнении.

[F12 saved replay](f12-saved-replay-2026-11-16.md) подтвердил selection miss: норма429 начинается на10289, выбранный фрагмент заканчивается10200. Current-code replay сохранённых draft/audit оставляет missing; это не новая генерация или повтор Qwen.

## Протокол живого сравнения

[Cases](../feasibility/product-options-cases-2026-11-16.json), [protocol](../feasibility/product-options-protocol-2026-11-16.json):6new questions (Python/SQLite/PostgreSQL, false premise, site/version, private no-public-evidence) и5knowncontrols. N06 с пустым gold оценивается как safe refusal отдельно, а не recall1. F03 в этом наборе требует также must-understand exception; знаменатели не объединяются с13ноября. Независимый gold preflight и hashes до первого inference. URL/gold не отправляются runtime; передаётся только вопрос.

Baseline46f8953; candidate runtime da750a0 (только missing-followup). Бинарники заморожены отдельно; последующие test-only commits не меняют эти binaries. Начальное накопленное состояние отказов взято из результата13ноября, не обнулялось. Caches изначально пустые и отдельные дляarms, состояние отказов общее накопительное. Серии последовательные baseline→candidate. Отдельная оценка источников обязательна: изменение retrievedbody не доказывает эффект одной ветви.

Первый запуск wrapper остановился в preflight из-за historical hardcoded2Harness assertion, до model/HTTP; новый scoped wrapper проверяет11cases безHarness и hashes. Это tooling deviation сохранено; trials не перезапускались и не перезаписывались.

## Результат

Все22attempts завершены. Независимая оценка:1/18→6/18requiredconditions,0/10→1/10completeanswers;false0→0,unknown5→5, НО новый unsupported generic J01/A1: gateFAILED. Нельзя считать одинаковое числоunknown отсутствием новыхошибок. Baseline:11/11attempts,1/18обязательныхусловий,0/10полныхответов,5unknown-as-cited,0false;N06ошибкаинструмента, не успешныйsafe refusal. Это независимаяразметка, не model.sufficient. Выпуск не выполнялся; global configs/API/модели/лимиты не менялись.

## Проверки и границы

Финальный offline набор после добавления inference baseline:175unit+6stdio passed,40unit+10stdio ignored. Fmt и clippy all-targets -D warnings прошли; release da750a0 собран,6stdio со свежим release прошли. Test-only additions после сборки не меняли runtime; hashes src/main.rs/research.rs/model.rs и binary проверены. Повторная compilation связана с отдельными worktree/profile build, не с новыми dependencies. Context lint безerrors,14warnings о старых открытых задачах; indexfresh и contextsmoke проходят.

Нормализованные evidence schema/section selector не публикуются как готовое улучшение. Общий quality gate не ослаблен. HTTP/source refusals сохраняются, ниодин blockedhost не повторяется после reset.

Live источники совпали целиком только3/11, общийbodyесть5/11. Qwen14→29calls,fallback5→0; knowncostUSD0.006587016→0.009279624; p50 38.870→27.498s. Это не доказательство ускорения: source/modelvariation. N04invalid_queryORwithsite, N05/N06source_query_mismatch сохранены, не safe-refusal successes. Новая generic J01/A1 делает candidate непригодным к объявлению принятой замены. Результаты: [mechanical](../feasibility/product-options-results-2026-11-16.json), [independentquality](../feasibility/product-options-quality-2026-11-16.json).

Независимый разбор query_forms/audit.next: ни в одном livecase не наблюдалась новая fallbackветвь; второй запрос совпадал с готовым modelnext. Поэтому эксперимент3 имеет только компонентное подтверждение, живой причинный эффект не доказан. Raw корпуса и checksum сохранены в [manifest](../feasibility/artifact-manifest.json), включая все ошибки и неудачные inference trials.
