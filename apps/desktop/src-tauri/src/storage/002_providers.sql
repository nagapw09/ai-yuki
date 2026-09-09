-- Миграция 2: заготовки подключений к провайдерам (ТЗ §4).
--
-- Строки создаются выключенными и без ключей: это не «уже настроено», а список
-- того, что Yuki умеет, чтобы в настройках не пришлось вводить адреса руками.
-- Ключи здесь не появляются никогда — только ссылка на запись в хранилище ОС.

INSERT OR IGNORE INTO providers (id, kind, label, base_url, default_model, secret_ref, is_default, enabled) VALUES
  ('anthropic', 'anthropic', 'Claude',      'https://api.anthropic.com',                          'claude-opus-5',   'provider:anthropic', 0, 0),
  ('openai',    'openai',    'OpenAI',      'https://api.openai.com/v1',                          'gpt-4.1',         'provider:openai',    0, 0),
  ('gemini',    'gemini',    'Gemini',      'https://generativelanguage.googleapis.com/v1beta',   'gemini-2.5-pro',  'provider:gemini',    0, 0),
  ('openrouter','openrouter','OpenRouter',  'https://openrouter.ai/api/v1',                       '',                'provider:openrouter',0, 0),
  ('xai',       'xai',       'xAI',         'https://api.x.ai/v1',                                'grok-4',          'provider:xai',       0, 0),
  ('ollama',    'ollama',    'Ollama',      'http://localhost:11434/v1',                          '',                NULL,                 0, 0),
  ('lmstudio',  'lmstudio',  'LM Studio',   'http://localhost:1234/v1',                           '',                NULL,                 0, 0);
