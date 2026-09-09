-- Схема базы Yuki (ТЗ §31).
--
-- Инвариант, который держится схемой, а не соглашением: API keys и токены сюда
-- не попадают. Таблицы хранят только ссылку на запись в OS secure storage
-- (Keychain на macOS, Credential Manager на Windows) — колонки `secret_ref`.
-- См. ТЗ §29 и §31.

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- Пользователь. Приложение персональное, но таблица нужна для профиля и языка (ТЗ §9, §36).
CREATE TABLE IF NOT EXISTS users (
  id            INTEGER PRIMARY KEY,
  display_name  TEXT    NOT NULL DEFAULT '',
  language      TEXT    NOT NULL DEFAULT 'ru',
  created_at    INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Настройки как key/value: набор ключей меняется быстрее, чем стоит менять схему.
CREATE TABLE IF NOT EXISTS settings (
  key        TEXT PRIMARY KEY,
  value      TEXT    NOT NULL,
  updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- AI-провайдеры (ТЗ §4). Ключ лежит в OS secure storage, здесь только ссылка.
CREATE TABLE IF NOT EXISTS providers (
  id            TEXT PRIMARY KEY,
  kind          TEXT    NOT NULL,
  label         TEXT    NOT NULL,
  base_url      TEXT,
  default_model TEXT,
  secret_ref    TEXT,
  is_default    INTEGER NOT NULL DEFAULT 0,
  enabled       INTEGER NOT NULL DEFAULT 1,
  created_at    INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE IF NOT EXISTS conversations (
  id         TEXT PRIMARY KEY,
  title      TEXT    NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL DEFAULT (unixepoch()),
  updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
  archived   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS messages (
  id              TEXT PRIMARY KEY,
  conversation_id TEXT    NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  role            TEXT    NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'tool')),
  content         TEXT    NOT NULL,
  -- Вложения, вызовы инструментов и метаданные модели — JSON, чтобы не плодить таблицы.
  metadata        TEXT,
  created_at      INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id, created_at);

-- Память (ТЗ §9). Пользователь может просмотреть, изменить и удалить любую запись.
CREATE TABLE IF NOT EXISTS memories (
  id         TEXT PRIMARY KEY,
  kind       TEXT    NOT NULL CHECK (kind IN ('short_term', 'session', 'long_term', 'episodic')),
  key        TEXT,
  content    TEXT    NOT NULL,
  source     TEXT,
  -- Эмбеддинг для векторного поиска; NULL, пока запись не проиндексирована.
  embedding  BLOB,
  confidence REAL    NOT NULL DEFAULT 1.0,
  -- Момент, после которого запись считается протухшей: обязателен для short_term.
  expires_at INTEGER,
  created_at INTEGER NOT NULL DEFAULT (unixepoch()),
  updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX IF NOT EXISTS idx_memories_kind ON memories(kind, updated_at DESC);

CREATE TABLE IF NOT EXISTS tools (
  id            TEXT PRIMARY KEY,
  name          TEXT    NOT NULL,
  description   TEXT    NOT NULL DEFAULT '',
  source        TEXT    NOT NULL DEFAULT 'builtin',
  capability_id TEXT REFERENCES capabilities(id) ON DELETE CASCADE,
  risk          TEXT    NOT NULL DEFAULT 'low' CHECK (risk IN ('low', 'medium', 'high')),
  enabled       INTEGER NOT NULL DEFAULT 1
);

-- Разрешения (ТЗ §21). `granted` — решение пользователя, `os_granted` — статус на уровне ОС.
CREATE TABLE IF NOT EXISTS permissions (
  category   TEXT PRIMARY KEY,
  granted    INTEGER NOT NULL DEFAULT 0,
  os_granted INTEGER NOT NULL DEFAULT 0,
  updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Capability (ТЗ §18).
CREATE TABLE IF NOT EXISTS capabilities (
  id          TEXT PRIMARY KEY,
  name        TEXT    NOT NULL,
  description TEXT    NOT NULL DEFAULT '',
  version     TEXT    NOT NULL DEFAULT '0.1.0',
  source      TEXT    NOT NULL CHECK (source IN ('builtin', 'mcp', 'plugin', 'user_script', 'api')),
  -- Полный манифест: permissions, tools, triggers, actions.
  manifest    TEXT    NOT NULL,
  enabled     INTEGER NOT NULL DEFAULT 1,
  health      TEXT    NOT NULL DEFAULT 'unknown' CHECK (health IN ('ok', 'degraded', 'failed', 'unknown')),
  health_note TEXT,
  installed_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE IF NOT EXISTS plugins (
  id           TEXT PRIMARY KEY,
  name         TEXT    NOT NULL,
  version      TEXT    NOT NULL DEFAULT '0.1.0',
  origin       TEXT    NOT NULL CHECK (origin IN ('local', 'git', 'dev_folder', 'generated')),
  location     TEXT    NOT NULL,
  manifest     TEXT    NOT NULL,
  enabled      INTEGER NOT NULL DEFAULT 0,
  installed_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- MCP-серверы (ТЗ §19).
CREATE TABLE IF NOT EXISTS mcp_servers (
  id          TEXT PRIMARY KEY,
  label       TEXT    NOT NULL,
  transport   TEXT    NOT NULL CHECK (transport IN ('stdio', 'sse', 'http')),
  command     TEXT,
  args        TEXT,
  url         TEXT,
  env         TEXT,
  secret_ref  TEXT,
  enabled     INTEGER NOT NULL DEFAULT 0,
  last_status TEXT    NOT NULL DEFAULT 'unknown',
  last_checked_at INTEGER
);

-- Пользовательские команды и автоматизации (ТЗ §16).
CREATE TABLE IF NOT EXISTS commands (
  id          TEXT PRIMARY KEY,
  name        TEXT    NOT NULL,
  description TEXT    NOT NULL DEFAULT '',
  phrase      TEXT,
  hotkey      TEXT,
  enabled     INTEGER NOT NULL DEFAULT 1,
  created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE IF NOT EXISTS command_nodes (
  id         TEXT PRIMARY KEY,
  command_id TEXT    NOT NULL REFERENCES commands(id) ON DELETE CASCADE,
  kind       TEXT    NOT NULL,
  config     TEXT    NOT NULL,
  next_id    TEXT,
  position   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_command_nodes_command ON command_nodes(command_id, position);

CREATE TABLE IF NOT EXISTS automations (
  id         TEXT PRIMARY KEY,
  name       TEXT    NOT NULL,
  trigger    TEXT    NOT NULL,
  definition TEXT    NOT NULL,
  enabled    INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE IF NOT EXISTS automation_runs (
  id            TEXT PRIMARY KEY,
  automation_id TEXT    NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
  status        TEXT    NOT NULL,
  started_at    INTEGER NOT NULL DEFAULT (unixepoch()),
  finished_at   INTEGER,
  error         TEXT
);

-- Журнал активности (ТЗ §23). Секреты сюда не пишутся ни в каком виде.
CREATE TABLE IF NOT EXISTS activity_logs (
  id          TEXT PRIMARY KEY,
  ts          INTEGER NOT NULL DEFAULT (unixepoch()),
  tool        TEXT    NOT NULL,
  target      TEXT,
  status      TEXT    NOT NULL CHECK (status IN ('ok', 'error', 'cancelled', 'denied')),
  result      TEXT,
  duration_ms INTEGER,
  task_id     TEXT
);
CREATE INDEX IF NOT EXISTS idx_activity_ts ON activity_logs(ts DESC);

CREATE TABLE IF NOT EXISTS notifications (
  id         TEXT PRIMARY KEY,
  title      TEXT    NOT NULL,
  body       TEXT    NOT NULL DEFAULT '',
  channel    TEXT    NOT NULL DEFAULT 'system',
  read_at    INTEGER,
  created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Задачи (ТЗ §32).
CREATE TABLE IF NOT EXISTS tasks (
  id           TEXT PRIMARY KEY,
  title        TEXT    NOT NULL,
  status       TEXT    NOT NULL CHECK (status IN ('queued', 'running', 'waiting_user', 'completed', 'failed', 'cancelled')),
  current_step TEXT,
  progress     REAL    NOT NULL DEFAULT 0.0,
  plan         TEXT,
  error        TEXT,
  created_at   INTEGER NOT NULL DEFAULT (unixepoch()),
  updated_at   INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status, updated_at DESC);

-- Напоминания (ТЗ §25).
CREATE TABLE IF NOT EXISTS reminders (
  id          TEXT PRIMARY KEY,
  text        TEXT    NOT NULL,
  due_at      INTEGER NOT NULL,
  recurrence  TEXT,
  completed_at INTEGER,
  created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX IF NOT EXISTS idx_reminders_due ON reminders(due_at) WHERE completed_at IS NULL;

-- Заметки. В ТЗ раздела нет, но §24 приводит сценарий «сохрани это в заметки»,
-- а у эталонного продукта заметки входят в базовый набор — см. docs/GAPS.md §5.
CREATE TABLE IF NOT EXISTS notes (
  id         TEXT PRIMARY KEY,
  title      TEXT    NOT NULL DEFAULT '',
  body       TEXT    NOT NULL DEFAULT '',
  pinned     INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL DEFAULT (unixepoch()),
  updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Категории разрешений из ТЗ §21 заводим сразу, чтобы UI не создавал их на лету.
INSERT OR IGNORE INTO permissions (category) VALUES
  ('microphone'), ('screen_recording'), ('accessibility'), ('files'),
  ('network'), ('shell'), ('camera'), ('notifications'),
  ('browser'), ('external_services');
