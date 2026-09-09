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

- [x] AI Provider Layer: Claude, OpenAI, Gemini + всё, что говорит на протоколе
      OpenAI — OpenRouter, xAI, Ollama, LM Studio, Custom API. Стриминг SSE,
      вызовы инструментов, `Test Connection` (§4)
- [x] Agent Loop целиком: PLAN → EXECUTE → OBSERVE → VERIFY (§5)
- [x] Chat как вторичный экран: markdown, блоки кода, статусы инструментов (§15)
- [x] Tools: приложения, окна, файлы, буфер обмена, клавиатура, мышь, экран (§6, §8)
- [x] Permission Gate + Confirmation modal с показом плана (§21, §22)
- [x] Activity Log (§23)
- [x] Memory: short-term / session / long-term / episodic + UI просмотра,
      правки, удаления и полной очистки; долгосрочные факты уходят в контекст
      запроса, краткосрочные протухают по сроку (§9)
- [x] Hotkeys, Notifications, Reminders: глобальный хоткей вызова, планировщик
      напоминаний с повторами, системные уведомления (§25)

Проверено на Windows 11 сквозным прогоном через локальный мок-провайдер:
стрим, сборка вызова инструмента из фрагментов SSE, выполнение в Rust, возврат
результата модели, подтверждение опасного действия — и отказ, и разрешение,
сохранение факта в память, срабатывание напоминания по сроку, глобальный хоткей
из свёрнутого состояния.

Не проверено визуально: сам toast на Windows. Он требует ярлыка с
AppUserModelID, которого нет у запуска из `target/debug` — проверять нужно
на установленной сборке.

## Фаза 2 — Голос и зрение (ТЗ §38)

- [x] Pipeline: Microphone → VAD → STT → Agent → TTS → Speaker (§10)
- [x] Push-to-talk, перебивание, синтез речи средствами ОС
- [~] Wake word «Yuki» / «Hey Yuki» — работает, но **не укладывается в 300 мс**
      из §37: фраза распознаётся целиком и только потом в ней ищется обращение,
      поэтому отзыв наступает после того, как пользователь договорил. Настоящие
      300 мс требуют отдельной модели пробуждения, слушающей поток непрерывно;
      это меняет только реализацию детектора, а не конвейер
- [ ] Streaming STT: сейчас фраза уходит на распознавание целиком
- [x] Screenshots + понимание экрана vision-моделью: снимок уходит модели
      картинкой во всех трёх диалектах, в журнал — только размеры (§6)
- [x] Accessibility tree как приоритетный источник, CV — fallback: на Windows
      через UI Automation; macOS ждёт живой проверки статуса TCC (§6)

Проверено на живом микрофоне утилитой `cargo run -p yuki-voice --example mic_check`
и на живом окне — `cargo run -p yuki-accessibility --example ui_check`.
Она же нашла и то, чего не видно из кода: фон тихой комнаты (RMS до 0.05) выше
любого разумного фиксированного порога, поэтому VAD считал шум речью непрерывно
и фраза не заканчивалась никогда. Пороги стали относительными — от нижнего
перцентиля громкости за две секунды, с прогревом и потолком, чтобы в шумной
комнате Yuki не оглохла.

Зрение проверено сквозным прогоном: снимок экрана на 408 КБ дошёл до провайдера
одной картинкой в правильном формате. Дерево интерфейса того же окна занимает
1155 символов вместо 408 КБ и содержит готовые действия — поэтому оно и стоит
в описаниях инструментов первым, а снимок объявлен запасным путём.

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
