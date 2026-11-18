# Первая публикация GitHub

Создан https://github.com/ukolov-dev/web-search: сначала private по умолчанию, затем PUBLIC по явному ответу пользователя. Анонимный GitHub API доступ проверен. Начальный чистый commit: e4e87f2ee4f72bb39fb85d12c873c0695a8b30d2, ветка main. Remote SHA проверен через ls-remote; это bootstrap нового пустого репозитория, не push в ранее существующую основную ветку. Рабочая ветка и история локальной разработки не переписывались; remote исходному research checkout намеренно не добавлен, чтобы не отправить приватную историю случайным push.

Публикация — отдельный очищенный snapshot исходного292b9b1. Исключены120служебных файлов: личные AGENTS, .codex, .agents, .project-context, Router tooling/config и локальная история. Из примераMCP и отчётныхJSON удалены личные абсолютные пути. Экспорт содержит374файла: runtime/tests/fixtures, research scripts/results/docs, новый README, CONTRIBUTING, SECURITY, CI и gitattributes с сохранением whitespace исходныхHTMLfixtures. Сырые корпуса, ключи, кэши и outputs/runs не публиковались.

README описывает инструменты, build/MCP startup, передачу данных внешнеймодели, лимиты и экспериментальнуюготовность. Лицензия не выбрана; разрешение на распространение исходников не заявлено. Metadata: Rust/MCP/search/experimental topics; squash-onlymerge и automaticbranchdelete. Ниrelease/tag, ниdeploy не создавались.

Проверен именно export:176unit+6stdio,42unit/10stdioignored; fmt и clippy --all-targets -D warningspassed. Проверка известных форматов ключей/privatekeys/личныхpaths и отсутствиеprivateinstructions passed. RawHTMLfixtures retained; whitespacechecks отключены толькодляtests/fixtures. Известные sourcequalitygates остаютсяfailed/partial и раскрытывREADME.

GitHubplugin использован для authenticatedlogin; создание repo инструментамиplugin недоступно, выполнено существующим gh. После создания private repository plugin fetch404 (installation permissions), поэтому remoteverificationчерезgh. Пользователю не требуется менять доступ для выполненнойпубликации.

CI: https://github.com/ukolov-dev/web-search/actions/runs/43862428820. На момент записи выполняется; не объявляется пройденным. Local export checkout: .cache/github-publication/repo, manifest/logs: .cache/github-publication. Эти generatedfiles не являются долговечнойпамятью или частьюпубликации.
