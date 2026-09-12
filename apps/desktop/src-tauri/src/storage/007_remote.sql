-- Удалённые устройства (ТЗ §28, docs/REMOTE-CONTROL.md).
--
-- Каждое устройство — своя запись со своим именем: общего пароля нет, и отзыв
-- одного не должен отключать остальные. `last_seen` нужен не для украшения
-- списка: по нему видно, что устройство, о котором человек забыл, продолжает
-- выходить на связь.
CREATE TABLE IF NOT EXISTS remote_devices (
    id         TEXT PRIMARY KEY,
    -- Канал, через который устройство подключено: сейчас только `telegram`,
    -- но своё приложение и relay из ТЗ §28 добавятся сюда же, и разделять
    -- таблицы значило бы дважды писать список и отзыв.
    channel    TEXT NOT NULL,
    name       TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    last_seen  INTEGER
);

CREATE INDEX IF NOT EXISTS idx_remote_devices_channel ON remote_devices(channel);
