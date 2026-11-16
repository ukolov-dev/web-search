# Project Context Router v0.11.0

Установлен **29.10.2026** только для web-search. В этом проекте прежней установки не было: выполнено первое подключение указанной версии. Отдельный исходный checkout Router с локальными изменениями и глобальная конфигурация Codex не менялись.

## Установка

- [Релиз v0.11.0](https://github.com/ukolov-dev/mcp-project-context-router/releases/tag/v0.11.0), пакет `mcp-project-context-router-0.11.0.tgz`.
- SHA-256: `358507670d57860747714ee05af6e538872bcc8d2b50e5afa943000e2f28e6f5`; совпал с SHA256SUMS и метаданными GitHub.
- Все 244 установленных файла пакета побайтно совпали с проверенным архивом.
- Версия закреплена URL релиза в package.json, integrity и зависимости — в package-lock.json. Установка выполнена с отключёнными lifecycle scripts; релиз содержит готовый dist.
- Node.js 22.23.2. Router — devDependency, не runtime-зависимость Rust-приложения.

Повторная установка зависимостей:

```sh
npm ci --ignore-scripts --no-audit --no-fund
npm run context:index
npm run context:doctor
```

## Подключение и индекс

Проектный MCP-конфиг: [.codex/config.toml](../.codex/config.toml), сервер `project_context`, профиль `core`. Относительный cwd указывает из .codex на корень проекта. Модули в .project-context/project.yaml (local development record; not distributed): core → src, tests → tests, docs → doc и README. Язык приложения Rust; команды проверки используют существующий Cargo.lock и локальный cargo-кэш.

Индекс после добавления документации построен: 300 символов, 0 context records. История прежних исследований осталась в doc/feasibility/runs; она автоматически не преобразовывалась в задачи и решения Router. Автоматические hooks и bundled skills не устанавливались. Настройки MCP из project config загружаются только в доверенном проекте: [официальная документация Codex](https://learn.chatgpt.com/docs/config-file/config-basic).

## Что проверено

- `codex mcp list --json` видит включённый project_context с проектным конфигом.
- Настоящий stdio-сеанс через MCP SDK: сервер объявляет версию **0.11.0**, 21 инструмент в core.
- `get_document_tree`: ограниченный список документов и nextOffset.
- `get_document_outline`: оглавление doc/local-mcp.md с ID разделов.
- `read_document_section`: чтение раздела в пределах 200 символов и продолжение с nextOffset.
- `doctor`: **0 ошибок, 5 предупреждений**. Индекс, зависимости, конфигурация и lint исправны.

Протокольные результаты — [mcp-smoke.json](../runs/router-0.11.0-2026-10-29/mcp-smoke.json), сводная проверка — [feasibility/context-router-0.11.0-2026-10-29.json](../feasibility/context-router-0.11.0-2026-10-29.json).

Предупреждения doctor сохранены: каталог не является Git-репозиторием, поэтому проверка Git ignore/tracking недоступна; coverage видит в том числе содержимое cargo-кэша; backlog пока пуст. Правила исключения добавлены в .gitignore, но Git-репозиторий автоматически не создавался. Не считать проверки Git ignore пройденными и не помещать приватные материалы в общедоступные для Router пути в расчёте только на .gitignore.

Текущий чат не переподключался: новая конфигурация ещё не доказывает доступность инструментов именно в нём. Для подхвата подключения нужен перезапуск/переподключение MCP-клиента, как указано в релизе. Функциональность подтверждена отдельным stdio-клиентом; Rust-приложение и VPS не изменялись и повторно не тестировались.
