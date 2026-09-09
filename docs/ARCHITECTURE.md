# Архитектура Yuki

Реализация ТЗ §4, §30, §43.

## Поток

```
┌─────────────────────────────────────────────┐
│  Desktop UI  (Tauri WebView · React + TS)   │  apps/desktop/src
│  Orbital screen · Chat · Capability Hub     │
└───────────────────┬─────────────────────────┘
                    │  invoke() / events
┌───────────────────▼─────────────────────────┐
│  Yuki Agent Core            (TypeScript)    │  core/
│  Planner · Memory · Context · Permissions   │
│  Tasks · Policies                           │
└───────┬───────────────────────────┬─────────┘
        │                           │
┌───────▼─────────┐        ┌────────▼─────────┐
│  Tool Registry  │        │ AI Provider Layer│  tools/ · ai/
│  browser · fs   │        │ OpenAI · Claude  │
│  apps · input   │        │ Gemini · xAI     │
│  screen · media │        │ OpenRouter       │
└───────┬─────────┘        │ Ollama · LM St.  │
        │                  └──────────────────┘
┌───────▼─────────────────────────────────────┐
│  Rust System Layer          (Tauri command) │  apps/desktop/src-tauri
└───────────────────┬─────────────────────────┘
                    │  SystemAdapter trait
        ┌───────────┴────────────┐
┌───────▼────────┐      ┌────────▼───────┐
│ WindowsAdapter │      │  MacOSAdapter  │      rust/windows · rust/macos
└────────────────┘      └────────────────┘
```

## Границы слоёв

| Слой | Язык | Знает о | Не знает о |
|---|---|---|---|
| UI | TS/React | состоянии агента, дизайн-системе | провайдерах, ОС |
| Agent Core | TS | tools, providers, memory, policies | React, Win/macOS API |
| Tool Registry | TS | Rust-командах через bridge | LLM-провайдерах |
| System Layer | Rust | адаптерах ОС | LLM, UI |
| Adapters | Rust | конкретной ОС | всём остальном |

Правило: **вниз по стрелке — можно, вверх — нельзя.** Agent Core не импортирует React;
Rust не знает про провайдеров.

## Agent Loop (ТЗ §5)

```
INPUT → UNDERSTAND → PLAN → TOOL SELECTION → EXECUTE
      → OBSERVE → VERIFY → NEXT STEP / COMPLETE → RESPONSE
```

Ключевой инвариант ТЗ §5 и §44: **Yuki не рапортует об успехе без подтверждения от
инструмента.** В коде это выражено типом — `ToolResult` не имеет варианта «наверное
получилось»: только `ok` с полезной нагрузкой или `error` с причиной. Шаг `VERIFY`
обязателен для всех действий с побочными эффектами.

## Permissions и Confirmation (ТЗ §21, §22)

Каждый инструмент декларирует `permissions: Permission[]` и `risk: RiskLevel`.
Перед выполнением `PermissionGate` проверяет:

1. выдано ли системное разрешение ОС (macOS Accessibility / Screen Recording);
2. разрешил ли пользователь категорию в политике Yuki;
3. требуется ли подтверждение по уровню риска — `LOW` авто, `MEDIUM` по настройке,
   `HIGH` всегда через модалку с планом действий.

## SystemAdapter (ТЗ §30)

Трейты в `rust/system`, реализации — в `rust/windows` и `rust/macos`, выбор через
`#[cfg(target_os = ...)]`. Ни один вызов из TS не идёт в платформенный код напрямую —
только через фасад `yuki_system::adapters()`.

## Хранилище (ТЗ §31)

SQLite-файл в app data dir. Схема — `apps/desktop/src-tauri/src/storage/schema.sql`,
миграции по номеру `user_version`. Секреты (API keys, tokens) в БД **не пишутся** —
только в OS secure storage через `keyring`; в таблицах лежат ссылки на записи.

## Маппинг каталогов на ТЗ §43

| Каталог | Раздел ТЗ |
|---|---|
| `apps/desktop/` | §13, §14, §15 |
| `core/` | §5, §9, §22, §24, §32, §33 |
| `rust/` | §6, §30 |
| `ai/` | §4 (AI Provider Layer), §26 |
| `voice/` | §10 |
| `tools/` | §6, §7, §8, §25, §35 |
| `automation/` | §16 |
| `capabilities/` | §17, §18 |
| `plugins/` | §20 |
| `mcp/` | §19 |
| `storage/` | §31 |
| `cloud/` | §28, §40 |
