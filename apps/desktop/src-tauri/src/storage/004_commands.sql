-- Миграция 4: тип триггера у команды (ТЗ §16).
--
-- Колонки phrase и hotkey уже есть, но по ним нельзя отличить «команда
-- запускается только вручную» от «фраза ещё не задана»: и там, и там NULL.
-- Разница существенная — от неё зависит, попадёт ли команда в список тех,
-- что перехватывают реплику пользователя.

ALTER TABLE commands ADD COLUMN trigger_kind TEXT NOT NULL DEFAULT 'manual';

-- Существующие строки: если фраза или сочетание заданы, тип выводится из них.
UPDATE commands SET trigger_kind = 'phrase' WHERE phrase IS NOT NULL AND phrase <> '';
UPDATE commands SET trigger_kind = 'hotkey'
WHERE trigger_kind = 'manual' AND hotkey IS NOT NULL AND hotkey <> '';
