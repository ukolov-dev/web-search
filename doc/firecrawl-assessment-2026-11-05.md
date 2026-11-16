# Firecrawl для web-search — оценка 5 ноября 2026

**Да: интересен для сравнения качества извлечения и как возможный self-hosted worker для сложных страниц. Прирост direct recall пока не измерен.** Рекомендуемый следующий этап — ограниченный component A/B; включение Firecrawl в runtime этим обзором не принято.

## Что проверено

Ограниченный инженерный обзор по запросу «это нам интересно?». Прочитаны официальные README, self-hosting/Cloud docs, Compose, лицензия, API manifest и браузерный путь; сопоставлены актуальные исходники нашего extractor/browser и отчёт 3 ноября. Код Firecrawl привязан к commit [`7d8e92dc5d270b1247d60e809c3fd32f52e0e47b`](https://github.com/firecrawl/firecrawl/tree/7d8e92dc5d270b1247d60e809c3fd32f52e0e47b). Официальный self-host guide описывает отдельный pinned release `v2.11.162`; не считаем его Compose тождественным checkout `main`.

Установок, контейнеров, Firecrawl API, product search/scrape, платных моделей и новых quality-экспериментов — 0. Все оценки ниже документальные; это не систематический литературный обзор и не qualification. [Протокол и область](../outputs/research/2026-11-05-firecrawl/research.md), [сохранённые ответы](../outputs/research/2026-11-05-firecrawl/sources/).

## Где есть польза

| Возможность | Применимость к нашему проекту | Статус |
| --- | --- | --- |
| Получение JS-страниц через Playwright, очистка HTML/Markdown | Кандидат на резервный reader после обычного HTTP; потенциально полезен там, где текста нет в исходном HTML | Поддержка документирована, наш выигрыш не измерен |
| Crawl/map и ссылки | Можно сравнить с нашим bounded discover/feed→article; наши четыре чтения и лимиты потребуют отдельного адаптера | Только кандидат для проверки |
| Rust SDK и MCP | Rust-приложение может обращаться к self-hosted API; MCP есть как клиентский интерфейс для агентов | Не доказывает качество поиска или отсутствие внешнего backend |
| Готовая служба на своём VPS | Возможная отдельная служба получения страниц; Rust остаётся владельцем query policy, дат, Qwen, coverage и audit | Архитектурное предложение, серверный этап не запускался |

Официальная документация включает core scrape/crawl/map/search и Fetch/Playwright в стандартную self-hosted поставку. LLM-backed форматы требуют своего провайдера. Agent/Browser/Interact отсутствуют в default stack; screenshots/actions требуют отдельного Fire-engine, который также не поставляется в нём. Возможности Cloud из общего README нельзя автоматически приписывать self-host. Источники: [self-hosted feature support](https://docs.firecrawl.dev/contributing/self-host#self-hosted-feature-support), [Open Source vs Cloud](https://docs.firecrawl.dev/contributing/open-source-or-cloud).

## Что он не решает автоматически

В [нашем отчёте 3 ноября](news-dates-2026-11-03.md) четыре baseline FN последнего NEW6 уже прошли article/date admission и Qwen: модель оставляла активное условие города missing. Отдельный известный portal FN имел полный прочитанный текст, но собственная `.news-one-date` в неразмеченном `div.news-one` не получила нужного type/date admission. Более чистый Markdown сам по себе не меняет эти правила.

Поэтому архитектурно полезная точка для пробы — получение/извлечение содержимого. Наши ownership публикационной даты, app/feed/type guards, exact quotes, source hashes/ranges, Qwen и обязательный audit продолжают оцениваться независимо. Firecrawl extraction не является oracle для direct или фактической истинности; его JSON/summary не становятся исходным свидетельством без проверки происхождения.

## HTML и происхождение свидетельств

Документация различает Markdown, очищенный `html`, `rawHtml` и `rawBase64`; последний заявлен как исходное HTTP body и запрашивается отдельно от остальных форматов. Источник: [Scrape formats](https://docs.firecrawl.dev/features/scrape#scrape-formats).

При этом в изученном браузерном пути HTML получается через `page.content()` после навигации/ожидания, а engine wrapper передаёт `response.content` как `html`. Это сериализованный DOM; его нельзя без отдельной проверки объявлять побайтным исходным HTTP body. Нужно явно различать полученные network bytes, rendered DOM и очищенный текст, сохранять final URL/status и проверять полноту redirect provenance. Реальное соответствие API-полей этим артефактам в self-hosted инстансе здесь не измерялось. Источники: [Playwright API](https://github.com/firecrawl/firecrawl/blob/7d8e92dc5d270b1247d60e809c3fd32f52e0e47b/apps/playwright-service-ts/api.ts#L271), [engine wrapper](https://github.com/firecrawl/firecrawl/blob/7d8e92dc5d270b1247d60e809c3fd32f52e0e47b/apps/api/src/scraper/scrapeURL/engines/playwright/index.ts#L37).

## Цена интеграции и границы

В проверенном [Compose](https://github.com/firecrawl/firecrawl/blob/7d8e92dc5d270b1247d60e809c3fd32f52e0e47b/docker-compose.yaml) API/worker запускаются через Node; рядом Playwright, Redis, RabbitMQ, NuQ PostgreSQL и службы опционального FoundationDB backend. API и Playwright имеют configured memory caps 8G и 4G: это **не** измеренная потребность, не минимальный размер VPS и не фактический RSS. Rust SDK — клиент к этому API, а не замена всего backend одним встраиваемым crate. Источники: [SELF_HOST](https://github.com/firecrawl/firecrawl/blob/7d8e92dc5d270b1247d60e809c3fd32f52e0e47b/SELF_HOST.md), [Rust SDK](https://docs.firecrawl.dev/sdks/rust).

Основная лицензия проекта — AGPL-3.0; README отдельно указывает MIT для SDK и некоторых UI-компонентов. Это необходимо учесть при выборе компонентов. В `apps/api/package.json` также встречается ISC metadata: она не является основанием считать весь репозиторий ISC. Юридические обязательства конкретного способа поставки не оценивались. Источники: [README licensing](https://github.com/firecrawl/firecrawl/blob/7d8e92dc5d270b1247d60e809c3fd32f52e0e47b/README.md#license), [LICENSE](https://github.com/firecrawl/firecrawl/blob/7d8e92dc5d270b1247d60e809c3fd32f52e0e47b/LICENSE).

По действующим правилам проекта Firecrawl Cloud API для search/scrape подключать нельзя. Self-hosted backend тоже требует отдельного архитектурного решения: Rust-runtime, ресурсы и сетевые границы уже заданы. Предварительная URL-проверка в Rust не заменяет контроль DNS/IP/редиректов/подресурсов в процессе, который реально выполняет браузерные запросы. У Firecrawl есть собственные security-проверки; их эквивалентность нашим требованиям не аудирована. Наш `src/browser.rs` остаётся deny-all диагностикой. CAPTCHA, блокировки, смена прокси/отпечатков не входят в предлагаемую пробу.

## Следующий шаг и условие решения

Первый кандидат — **self-hosted scrape как экспериментальный reader**. На одной фиксированной версии сравнить с нашим extractor 8–12 реальных страниц: техническая документация и новости, простые положительные страницы, пропуски extraction, feed/app/неверная дата как отрицательные контроли. До получения новых данных заморозить список, исходные snapshots, необходимые собственные body/date witnesses и метрики. Проверять сохранность свидетелей и даты/owner/type, загрязнение чужими teaser, затем direct recall/new FP/unknown и затраты. Увеличение длины Markdown не считать улучшением.

Публичный JS-прогон допустим после отдельной сетевой квалификации worker; синтетическая deny-all проверка её не заменяет. Результаты Firecrawl измерять отдельно от исторических NEW12/supplement/NEW6. Если extraction не восстанавливает пригодные свидетельства или создаёт новые ложные direct, подключение к runtime не обосновано.

Статус: **запрошенная оценка завершена; кандидат перспективен для component A/B, продуктовая польза не подтверждена**. ТЗ, runtime, модели, MCP-конфиги и VPS по этому обзору не менялись.
