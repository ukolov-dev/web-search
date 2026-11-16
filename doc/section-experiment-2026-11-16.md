# Эксперимент1: секция длинного RFC до обрезки

На исходном сохранённом HTML F02 prototype восстанавливает норму о content+trailers: prefix100000 (truncated=true, нормы нет) →1660 символов раздела15.3.5 (truncated=false, норма есть). Это offline extraction outcome, не model answer или live retrieval.

База ebdcc83, изолированная work/research-section-experiment. Изменения только test module src/main.rs и fixture/report; production API/extraction callers не изменены. Task root TASK-20261116-110523-001. Сначала прочитаны SOURCE_OF_TRUTH, план16.11, Router scoped context; просмотрены extract→extract_cached→extract_uncached и caller research Backend.read. Текущий extractor не получает вопрос; интеграция потребует внутреннего question-aware пути и query/selection version в ключе кэша, без нового публичного параметра.

Прототип допускает выбор только после существующей полной проверки declared modern RFC (URL, metadata, header). Удаляет nav/aside/footer/hidden из candidate tree, ищет настоящий section с единственным heading, считает точные совпадения терминов вопроса с heading. Единственный лучший раздел выбирается с оригинальными h1 идентификации; отсутствие/равенство scores сохраняет прежний prefix. Никакого hardcode204/9110 в селекторе, новых моделей/HTTP/лимитов нет. Функция только cfg(test); compiled runtime не меняется.

Сокращённый текст имеет собственный text_hash, content_scope=experimental_selected_rfc_section, selected_section и offset_scope: координаты Unicode относятся к выбранному extracted text, **не** исходному полному body. Original DOM section копируется без переписывания цитат. Mapping в полный документ не заявляется. Downstream document_version должен вычисляться на этом text_hash как у обычного extraction; это ещё не интегрировано.

Проверки: original captured F02 HTML; bounded fixture с тем же реальным15.3.5 и вставляемым длинным prefix; TOC/sidebar decoys; неверный домен/RFC metadata/header; неизвестное слово; равные heading scores; другой номер RFC и другой статус777; no-store RFC9111 сохраняется. Prefix assertion показывает исходную потерю, candidate assertion — восстановление. Первый вариант :scope selector не нашёл heading (no-effect trial), заменён на единственный descendant heading; этот ложный старт не выдаётся за успех.

Команды и точные результаты в [JSON](../feasibility/section-experiment-2026-11-16.json). Full offline suite, fmt, clippy прошли; отдельный ignored captured replay passed1. Исходный large HTML и baseline/candidate JSON лежат в ignored outputs/section-experiment-captured. Root может архивировать их отдельно.

Ограничения: lexical exact heading match не умеет semantic aliases; набор малый, другой длинный RFC — синтетический контроль, не live источник. Слишком общий/неоднозначный запрос вернёт prefix и останется miss. Выбранная секция может не включать cross-section exceptions, поэтому исходный question coverage/audit обязателен. Деплой, модельные вызовы и production wiring не выполнялись.
