# Seznam/Naver: первый нативный этап

**29.10.2026.** По переданному исследованию B17 добавлены экспериментальные HTML-адаптеры `seznam` и `naver` в Rust CLI и MCP `search`. Перенесён слой запроса/разбора, а не вся исследовательская policy. Внешний пакет исследования не изменялся.

## Что работает в Rust

```sh
# Живые обращения; здесь приведены как примеры, в этом этапе не запускались.
./target/release/web-search search seznam 'Rust ownership borrowing documentation'
./target/release/web-search search naver 'Rust ownership borrowing documentation'
```

MCP: `search` с явным `engine=seznam` или `engine=naver`; прежние `query`, `max_results` и site-проверка сохранены. Нового значения по умолчанию, automatic fallback и обхода блокировок нет. Общий HTTP-клиент сохраняет публичные URL/DNS/IP/peer/redirect проверки, лимиты и truthful User-Agent.

- Seznam: `https://search.seznam.cz/?q=...`, органические `a[data-e-a=heading]`. Сниппет — `.e69e8d` из ближайшего предка, содержащего ровно один organic heading. Текст соседней карточки не заимствуется; отсутствие сниппета остаётся пустой строкой.
- Naver: `https://search.naver.com/search.naver?where=web&query=...`, `a[data-nlog-area$='.link']`. Как в B17, сохраняется первый порядок URL, но берётся **последний** подходящий anchor text этого URL. Он возвращается и как title, и как snippet; может оказаться описанием/датой, а не заголовком. Search предупреждает об этом. Не выдаём его за отдельно ограниченный сниппет.
- Оба адаптера сохраняют первые десять уникальных допустимых URL. Относительные navigation href не становятся результатами. Общая нормализация снимает fragment/tracking; site-фильтр применяется после разбора.
- Naver обновляет повторные anchors только для первых десяти URL, не накапливает весь список. Seznam ограничивает поиск предка 64 уровнями; на более глубокой неизвестной структуре сниппет остаётся пустым. Это ресурсные границы native parser; на corpus B17 они не изменили текст/порядок.
- Challenge guard исключает organic anchors/Seznam snippets и scripts/styles/templates при анализе окружающего текста. Реальная verification UI и виджеты CAPTCHA дают `source_challenge`; MCP отключает источник в текущем процессе. Упоминание CAPTCHA в органическом результате не блокирует его. Неизвестная разметка без явного no-results маркера остаётся `parser_changed`.

## Что проверено на B17

Первичный пакет: sibling project `research/outputs/research/2026-10-29-continuous-web-search/experiments/round-30/B17`. Чтение выполнено локально, без импорта его сетевых helpers. [Манифест импорта](../feasibility/b17-import-2026-10-29.json) содержит проверенные 12 frozen module/schedule hashes, commitment `fresh/enriched.json` и SHA-256 всех 62 HTML.

Исходный `feasibility/search-cases.json` совпал с original20 исследования: SHA-256 `b67e3269830ae0e976aa9b4c5dc2f6b9c3e922ad1adda35738289d6b1b8325d6`. Набор не редактировался. Старые predictions, gold и model scores в parser replay не читались.

**Replay всех 62 страниц прошёл:** после общей нормализации нет отличий порядка URL и текста результатов относительно fresh/enriched. 61 страница распознана; R034 Naver содержит ноль подходящих organic anchors, а также AI-блок/иной layout. B17 извлёк ноль, Rust возвращает `parser_changed`. Это намеренная граница fail-closed, а не подтверждённая пустая выдача всего веба. Отдельных model/quality оценок Rust здесь не вычислял.

Воспроизведение с явно указанным каталогом B17:

```sh
WEB_SEARCH_RESEARCH_B17_DIR=/path/to/B17 CARGO_HOME="$PWD/.cache/cargo" cargo test --locked --offline search::tests::replay_b17_source_captures -- --ignored --exact --nocapture
```

Перед replay проверяются frozen files, enriched commitment и каждый HTML-хеш. Обычные тесты не требуют sibling project. Полный corpus остаётся в исходном research-пакете, в web-search сохранён компактный манифест. Без него расширенный replay недоступен; синтетические тесты и приложение продолжают работать.

Другие проверки: **19 обычных тестов прошли**, 12 ignored; fmt, clippy all-targets с `-D warnings`, release-сборка и offline stdio проверки release-MCP. В тестах проверены граница сниппета/соседней карточки, duplicate-anchor правило Naver, query encoding, неизвестная разметка, challenge и circuit состояния обоих новых engine. Сводка — [машинные результаты](../feasibility/native-search-results-2026-10-29.json).

## Значение переданных 16/20

У отправителя B17 сообщены 16/20 за один день 29.10: две оценки по 16, strict intersection sensitivity 15, C28 adjudication обосновывает 16. Эти числа не являются нашим новым native quality measurement.

Результат относится к объединённой policy Seznam+Naver с ограниченными формулировками, local MiMo9B classify-all/direct-first и fresh RRF. В этом этапе **не перенесены** query scheduling/translation/cycleF3, модельный протокол/контекстный бюджет, классификатор, RRF и target extraction/full answer. Их нельзя заменить двумя вызовами нового `search` и заявить native16/20.

Нативный транспорт также отличается от research urllib: используется существующий проверяющий HTTP-клиент с собственным User-Agent и интервалом не менее 2 с, а у frozen capture — 2,1 с. Offline parity не доказывает одинаковую живую выдачу с этим транспортом. В этом этапе нет новых GET, вызовов oMLX, JCAR retries, live failover или Chromium. Target HTTP/body остаются неизвестными.

## Следующая календарная проверка

Сегодня 29.10 — день2 не наступил. Проверен исходный `day2.py --date 2026-10-29` **без --live**: отказ с same-day guard до создания серии/сети. Новый каталог не создан. Часы/дата не подменялись, автоматизация не создавалась. Хеш wrapper и результат — в машинной сводке.

В реально другой календарный день, с фактической датой машины, согласованной с Europe/Moscow, разрешён существующий frozen reproducer:

```sh
python3 "$WEB_SEARCH_RESEARCH_B17_DIR/day2.py" --date YYYY-MM-DD --live
```

Он проверяет хеши/дату/отсутствие outdir и выполняет только source capture → parse/enrich. Затем нужны новые независимые метки и тот же frozen classifier/export/RRF. Не импортировать старые captures/gold и не складывать лучшие вопросы разных серий. Отказ review/CAPTCHA не обходить; изменение источника оценивать честно.

Даже успешный day2 combined score не является автоматически проверкой каждого engine как main/reserve, fault failover, Rust native E2E, target availability или всего ТЗ. Параллельно подготовлен только первый нативный слой; квалификация источников и независимая acceptance остаются открыты.
