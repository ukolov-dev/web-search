# F12: сохранённый контекст на текущем runtime

Offline replay на8f6c86a прошёл:1test,0fail. Это диагностика сохранённых draft/audit13ноября, без HTTP и без новой генерации модели.

RFC6585 body содержит общий optional clause на offset3050 и собственный429 not-required clause на10289. Ни один не попал в saved selected chunks. S1P2=[9000,10200) заканчивается перед нужной нормой. Слово optional в нём относится к428 (§7.1), а не429: считать его evidence для429 было бы ошибкой.

Текущий select_quotes→prune_invalid_claims→audit_context→apply_audit принимает четыре прежних фоновых claim и сохраняет missing[0,1], sufficient=false. Первый доказанный разрыв: body→selected context; вторая генерация повторяла фон, audit честно оставлял неполноту. Это не доказательство текущего extraction/Qwen или новой модели: они здесь не запускались.

[Результат и точная команда](../feasibility/f12-saved-replay-2026-11-16.json). Raw fixture, временный test и log сохранены в outputs/f12-saved-replay-2026-11-16 основного checkout с SHA256. В commit только отчёт/результат: production и обычная suite не менялись. Не проведены новая model regeneration, независимая приёмка и live qualification.
