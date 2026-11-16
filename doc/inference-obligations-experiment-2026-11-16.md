# Эксперимент2: нормализованные доказательства inference

Кандидат не принят для runtime: связывание rule/input/result помогает диагностике, но не гарантирует semantic support. Вся реализация test-only; production audit и схема продукта не менялись.

Заморожены исходные J01 question/claim/quotes из round2 candidate model-02, контрфактические number/entity/version/negation и non-Option Vec::pop. Generic schema требует own evidence index+verbatim rule, exact question input spans, полный result и verdict. Детерминированная проверка устанавливает наличие этих связей, но не доказывает смысл правила. API-specific whitelist не вводился.

Round1:10cases,8/10 совпали с gold, оба positives потеряны из-за перефразирования result; формальный binding отверг все ответы. Отдельный judge ошибочно поддерживал negative cases. Round2:уточнено требование exact-copy result, тот же development10:8/10 совпали, positives восстановлены, но новые false accepts J01_entity и J01_rule_removed. При удалении general rule модель назвала общим правилом частный car/bike example. Значит structural binding не замыкает доказательство.

После round2 без новых prompt changes заморожен отдельный transfer4:две toy Python mapping задачи и две задачи по synthetic protocol version rule. 4/4 совпали. Это узкий положительный сигнал, который не отменяет failed J01 gates; не production benchmark и не RFC qualification.

Ошибка запуска:попытка transfer до изменения harness фактически повторяла development fixture. Запуск остановлен; raw сохранён в transfer/ и исключён из сравнений. Одна незавершённая попытка имеет неизвестную стоимость. Стоимость завершённых вызовов и количество записаны в [results](../feasibility/inference-obligations-results-2026-11-16.json), там же SHA-256 локального ignored raw archive. Gold не передавался модели.

Проверки: `CARGO_HOME="$PWD/.cache/cargo" cargo test --locked --offline` —171unit+6stdio; `cargo fmt --check` и `CARGO_HOME="$PWD/.cache/cargo" cargo clippy --locked --offline --all-targets -- -D warnings` проходят. Адресный live harness: `RESEARCH_INFERENCE_DIR="$PWD/outputs/inference-experiment/run2" CARGO_HOME="$PWD/.cache/cargo" cargo test --locked --offline normalized_inference_frozen_audit -- --ignored --nocapture`; transfer дополнительно использует `RESEARCH_INFERENCE_FIXTURE="$PWD/tests/fixtures/research/inference-transfer.json"` и другой новый output directory. Сохраняет один запрос на case без retries; не читает public sources, не меняет global configs.

Следующий вывод:не интегрировать этот кандидат как защиту и не смягчать existing guards. Для эксперимента требуется оценка семантики самих bound rule fragments; один дополнительный self-verdict проблему не решает. Результаты исторических прогонов сохранены отдельно.

## Позднее парное baseline сравнение

На тех же неизменных10cases/gold выполнены ещё10serial calls с оригинальным AUDIT_PROMPT из ebdcc83 и схемой Audit. Снимок prompt+cases до inference сохранён. Это component comparison: исходные claim/question/own-quotes, generic inference rationale, без rejection background; не полный runtime replay и не рандомизированное сравнение. Candidate — прежний round2, он не перезапускался.

Baseline:8/10, ноль false accepts, оба positives отклонены. Candidate:8/10, оба positives восстановлены, но появились два false accepts — J01_entity и J01_rule_removed. Поэтому равный средний score скрывает ухудшение безопасности; кандидат по-прежнему FAILED. Старый audit требует частного примера7/9 несмотря на общий rule; новая нормализация разрешает некоторые выводы, но не обеспечивает собственную опору каждого claim.

[Подробная таблица, стоимость10calls и checksum baseline raw](../feasibility/inference-baseline-results-2026-11-16.json). Старые результаты/ошибочные прогоны не изменялись. Дополнительные проверки:fmt/clippy прошли; harness10calls завершился без retries/HTTP отказов.
