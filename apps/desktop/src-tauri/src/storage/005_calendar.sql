-- Миграция 5: подключённые календари (ТЗ §25).
--
-- Секретов здесь нет и быть не может: client_secret и refresh-токен лежат в
-- хранилище ОС (ТЗ §29), а access-токен не сохраняется вовсе — он живёт час и
-- запрашивается заново по refresh-токену. В базе остаётся только то, что не
-- даёт доступа само по себе: какой сервис, какое приложение и когда подключено.

CREATE TABLE IF NOT EXISTS calendar_accounts (
  provider     TEXT PRIMARY KEY CHECK (provider IN ('google', 'microsoft')),
  -- client_id заводит пользователь в консоли сервиса: вшить учётные данные
  -- приложения в открытый репозиторий нельзя, они мгновенно перестают быть его.
  client_id    TEXT    NOT NULL,
  connected_at INTEGER,
  updated_at   INTEGER NOT NULL DEFAULT (unixepoch())
);
