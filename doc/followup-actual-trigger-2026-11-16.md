# Реальный duplicate followup: same-state decision replay

На runtime bda65f3 подтверждён реальный trigger N01 из06.11: первая форма `RFC 6585 428 Precondition Required caching MUST NOT`, первый draft пуст и предлагает в точности тот же запрос. Это реальные сохранённые model outputs, не синтетическая подмена. Фиксированы исходный вопрос/plan/model02context/output/missing и текущий cumulative refusal ledger до любых новых вызовов.

Из882 сохранённых ответов найдено16 naive duplicate-next; большинство относится к последнему round или неиспользуемому draft.next и не доказывает активную ветвь. Два первых пустых draft — N01 и N12. Фокус выбран заранее N01; closed-facet trigger среди просмотренных outputs не найден.

Один explicit offline test проходит: Session::query→search на записанном next возвращает false без backendcall (dedup), candidate followup_query формирует initial query+missing facet. Формы1, SERP1, documents2, generations2 и claims0 не изменились до остановки. Никаких новых model/HTTP/Qwen/cache запросов. Текст missing facet — вопрос о кэшировании428, а не ответ из gold.

Остановка — перед новым SERP: ответ на focused query отсутствует в transcript; старый SERP не переименовывается. Точная исходная queue не сохранена, поэтому целиком controller не воспроизводился. Исторический baseline продолжил читать уже найденную очередь и завершился budget_exhausted: нельзя утверждать, что именно этот baseline мгновенно остановился. Здесь доказаны реальный trigger и разница решения, а не улучшение ответа.

[Результат, команды и SHA256](../feasibility/followup-actual-trigger-2026-11-16.json). Raw frozen state/reproducer/log сохранены в root runs/product-options-followup-2026-11-16/actual-N01. В commit только отчёт/результат, production и тестовая suite неизменны. Parent отдельно решает запуск focused-query live probe в остатке бюджета. No live; общий adaptive retrieval и end-to-end gain не подтверждены.
