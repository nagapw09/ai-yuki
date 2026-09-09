# Роадмап

Порядок фаз выведен из ТЗ §38–§40. Каждая фаза заканчивается работающим приложением,
а не «слоем без UI».

## Фаза 0 — Каркас ✅

- [x] Монорепо по структуре ТЗ §43 (npm workspaces + cargo workspace)
- [x] Tauri 2 shell, собирается на Windows и macOS
- [x] Design System: токены, типографика, 4/8px сетка, motion (§14)
- [x] Orbital-экран: Orb со всеми состояниями, Command Bar, rail (§13)
- [x] `SystemAdapter` / `FileAdapter` / `InputAdapter` / `ScreenAdapter` — трейты + обе реализации (§30)
- [x] SQLite-схема на все таблицы §31, секреты в OS secure storage (§29)
- [x] Типы ядра: Tool, ToolResult, Permission, RiskLevel, Task, Capability (§5, §21, §22, §32)
- [x] i18n ru/en/uk (§36)

## Фаза 1 — MVP-ядро (ТЗ §38)

- [ ] AI Provider Layer: OpenAI, Claude, Gemini + streaming (§4)
- [ ] Agent Loop целиком: PLAN → EXECUTE → OBSERVE → VERIFY (§5)
- [ ] Chat как вторичный экран: markdown, code blocks, tool status (§15)
- [ ] Tools: apps, filesystem, clipboard, keyboard, mouse (§6, §8)
- [ ] Permission Gate + Confirmation modal с показом плана (§21, §22)
- [ ] Activity Log с редактированием секретов (§23)
- [ ] Memory: short-term / session / long-term / episodic + UI управления (§9)
- [ ] Hotkeys, Notifications, Reminders (§25)

## Фаза 2 — Голос и зрение (ТЗ §38)

- [ ] Pipeline: Microphone → VAD → STT → Agent → TTS → Speaker (§10)
- [ ] Wake word «Yuki» / «Hey Yuki», latency < 300 мс (§10, §37)
- [ ] Push-to-talk, interruption, streaming STT
- [ ] Screenshots + базовое понимание экрана vision-моделью (§6)
- [ ] Accessibility tree как приоритетный источник, CV — fallback (§6)

## Фаза 3 — Расширяемость (ТЗ §17–§20, §38)

- [ ] Capability Manager: манифест, health status, sandbox, validation (§18)
- [ ] Capability Hub UI: Installed / Available / Add Tool / Add MCP / Add API (§17)
- [ ] MCP Manager: транспорты, auth, Test Connection (§19)
- [ ] Сценарий «Юки, добавь возможность управлять Spotify» целиком (§17, §41.8)
- [ ] Browser Agent на Playwright (§7)
- [ ] Basic automation: triggers, actions, if/else (§16)

## Фаза 4 — MVP-2 (ТЗ §39)

- [ ] Visual Command Builder, advanced automation
- [ ] Ollama + LM Studio, Local Only режим (§29)
- [ ] Plugin SDK и self-extension workflow (§20, §18)
- [ ] Calendar: Google, Outlook (§25)
- [ ] VRM avatar со всеми состояниями и lip-sync (§12)
- [ ] Advanced memory, advanced computer vision

## Фаза 5 — Version 2 (ТЗ §40)

- [ ] Remote control: Mobile/Web → Secure Gateway → Desktop, pairing по QR (§28)
- [ ] Telegram, cloud sync, proactive assistant (§34)
- [ ] Coding-agent integrations (§27)

## Критерии готовности MVP (ТЗ §41)

Сценарии, которые должны стабильно проходить на Windows **и** macOS:

1. «Юки, открой браузер и найди музыку для концентрации»
2. «Найди последний PDF в Downloads и открой его»
3. «Напомни мне завтра в 10:00 позвонить Ивану»
4. «Переведи выделенный текст»
5. «Запусти мой рабочий режим»
6. «Найди файл и перемести его в папку X»
7. «Открой приложение и выполни действие внутри него»
8. «Юки, добавь возможность управлять Spotify»

Бюджеты §37: startup < 3 с · wake word < 300 мс · first token < 1.5–2 с · UI 60 FPS.
