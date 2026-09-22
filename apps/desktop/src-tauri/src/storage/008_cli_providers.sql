-- CLI owns login and credentials. Never copy its OAuth tokens into Yuki.
INSERT OR IGNORE INTO providers (id, kind, label, base_url, default_model, is_default, enabled) VALUES
  ('claude_cli', 'claude_cli', 'Claude Code · подписка', 'https://claude.ai', 'default', 0, 0),
  ('codex_cli', 'codex_cli', 'Codex · подписка ChatGPT', 'https://chatgpt.com', 'default', 0, 0);
