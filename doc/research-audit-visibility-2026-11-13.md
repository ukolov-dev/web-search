# Границы видимости конфликтов при quote-bound audit

Независимый reviewer воспроизвёл ошибку нового audit context на допустимых границах:12chunks×1200,4sources,4claims и вход до24000 символов. После удаления полного chunk из rejection_context его ID оставался видимым ради обязательной цитаты. Это позволяло принять другую конфликтную цитату из скрытой части того же chunk.

Исправление разделяет видимость mandatory witnesses и полных rejection chunks. Конфликт разрешён только для показанного полного chunk либо для точного уже показанного witness (source/version/Unicode range/text). Обязательные support quotes не теряются. Внутренний chunk_id добавляется только во вход аудита; публичная форма witness сохранена.

Регрессия reviewer_bounded_audit_unseen_conflict_quote: baseline exit101 с предсказанной ошибкой; исправление проходит и отдельно сохраняет допустимость точной mandatory цитаты из обрезанного chunk. Полный локальный набор166unit+6stdio проходит. Первое применение patch пропустило один legacy caller; ошибка компиляции исправлена, не засчитывалась как red-reproduction.

[Команды и снимок](../feasibility/research-audit-visibility-2026-11-13.json). Модель/HTTP этим тестом не вызываются. Свойство guard не является общей приёмкой качества research.
