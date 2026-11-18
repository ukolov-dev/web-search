# Исправления audit reason и URL-исключений

Обе ошибки воспроизведены и исправлены: **FIX_PROVEN**. Implementation snapshot `e075a93`, отдельная ветка `work/audit-reason-url-exclusions`. Сетевые, LLM и Qwen вызовы не выполнялись; остановленные кампании не возобновлялись, их файлы не изменялись.

## Audit: метаданные не отбрасывают проверенный ответ

Сохранённый audit C01 содержал корректный JSON и4supported verdicts, но reasonA4 имел201символ при лимите200. До исправления настоящий `Session.apply_audit` возвращал `audit reason bounds`. Та же адресная команда после исправления проходит: exit101→0.

JSON schema `Check.reason` теперь явно задаёт minLength1/maxLength200. Runtime сначала проверяет всю исходную строку на непустой содержательный текст и control characters, затем при необходимости убирает крайние пробелы и ограничивает пояснение до200Unicodeсимволов. Событие `audit_reason_truncated` содержит claim ID, исходную и сохранённую длины. Исходный model capture сохраняет полный reason201, его SHA не изменился. Check.reason не участвует в semantic verdict; `Conflict.reason` и все witness/ID/verdict/conflict/sufficiency проверки остались строгими.

Отдельные регрессии проверяют Unicode, пробельный префикс, control character после201символа, неизвестный ID, невалидный verdict, неверную цитату и unsupported verdict. Нормализация не делает такие случаи supported.

Offline replay фактического C01 восстановил источники, версии, SHA, chunks и собственные цитаты через прежний strict restorer, затем применил исходные draft и audit. Получены прежние4текста claims и accepted audit; результат всё ещё partial: цитируется1документ при требовании2. Это отдельное component evidence, не пересчёт исторической серии и не прирост поиска. Использован existing saved Session loader с нулевыми transport counters и metadata placeholders; они не выдаются за свежий HTTP/cache/MCP результат. Первое обращение к тесту с коротким `--exact` выбрало0тестов; оно не засчитано. Полное имя выбрало1offline test, он прошёл.

## URL-исключения и hard domain refusals

В test-only source-aware протоколе timeout/404/5xx теперь сохраняются в `document_url_blocks`, отдельно от `document_blocks`. Идентификатор URL сохраняет path, query и trailing slash, убирает fragment и использует обычную URL-нормализацию. Проверка выполняется до initial/redirect/cache-hop DNS/HTTP. Другой path или query того же host не исключается автоматически.

CAPTCHA/challenge и403/429 документа сохраняют доменный запрет для фактически отказавшего конечного host. Начальный host редиректа не наследует чужой отказ. Прежние hard reasons не переписываются. Ошибки модели/Qwen, неверный URL и отмена сохраняют fatal halt; document recovery не снимает их. Сериализация ledger сохраняет неизвестные/исторические поля.

Подготовка нового seed переносит **только два подтверждённых исторических timeout**: docs.python.org → `/el/3/glossary.html`, www.pythonpool.com → `/pythons-itertools-groupby/`. Требуются точное совпадение текущей soft-host записи и сохранённое свидетельство URL failure. Исходная запись переносится в `legacy_timeout_host_exclusions`; failed URL остаётся запрещён. Неподтверждённый timeout, изменённый403 и отсутствие provenance не разрешают снять host block. Повторная подготовка идемпотентна. Та же проверка настоящей prepare boundary: exit1→0; добавлены отрицательные случаи.

Новый seed подготовлен только как reviewable preview в `runs/audit-url-fix-2026-11-18/seed-preview.json`. Старые source-state/results/captures не переписаны, новый live process с ним не запускался. Source-aware/priority режимы по-прежнему test-only. Нормализация audit — изменение runtime-кода research; новый binary не деплоился и глобальные MCP процессы не перезапускались.

## Проверки и доказательства

202unit+6stdio passed;49unit/10stdio ignored. Fmt, all-targets clippy-Dwarnings, release build, оба Python self-check и адресные guards прошли. Ещё6stdio прошли со свежим release binary. Release SHA256 `d239d71d4ac630fb35234597ecf035974d794e8f7bd9d6f26f7f39d0d21a6b90`. Все проверки выполнялись на реализации e075a93; последующие изменения — документы/evidence.

[Compact results](../feasibility/audit-url-fix-results-2026-11-18.json). Red/green logs, actual restored fixture/replay и seed preview: `feasibility/audit-url-fix-corpus-2026-11-18.tar.gz`, [размер/SHA](../feasibility/artifact-manifest.json). Ключей, trusted settings и binary в архиве нет. Локальный native bug report: `outputs/bug-reproducer-report.md` (ignored artifact).

Живое качество кандидата, новая полная серия, общая qualification, CI/publication и deploy не проверены. Исправление этих двух ошибок не означает, что остальные причины пропусков или ошибок поиска устранены.
