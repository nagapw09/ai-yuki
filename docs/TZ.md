# Техническое задание: Yuki (v2.0, Windows + macOS)

> Исходный документ: `Yuki_TZ_v2.0_Windows_macOS.docx`. Текст приведён без изменений — это источник истины для роадмапа.

Персональный AI Voice/Desktop Assistant
Версия 2.0 — Windows + macOS
Основные изменения: кроссплатформенная архитектура Windows + macOS; стек Rust + Tauri + React/TypeScript; вместо публичного Marketplace — Personal Capability Hub с возможностью добавлять функции вручную или просить Yuki сделать это; полностью переработанная концепция главного интерфейса.
1. Концепция
Yuki — персональный AI-компаньон, который живёт на компьютере пользователя. Она разговаривает естественным языком, управляет приложениями, браузером, файлами и системой, выполняет многошаговые задачи, запоминает предпочтения и создаёт автоматизации.
Принцип: голос/текст → понимание → планирование → инструменты → действие → наблюдение → проверка → результат.
2. Платформы
Обязательная поддержка:
• Windows 10/11 x64.
• macOS 13+ на Intel и Apple Silicon.
Core-логика должна быть общей для обеих ОС. Платформенные функции реализуются через SystemAdapter:
SystemAdapter → WindowsAdapter / MacOSAdapter.
3. Технологический стек
Рекомендуемый стек:
• Desktop: Rust + Tauri 2.x.
• UI: React + TypeScript.
• AI/Agent: Python 3.12+ как отдельный локальный service layer.
• Storage: SQLite + локальный vector layer.
• Cloud — только для будущих remote/sync функций.
Tauri выбран вместо Electron из-за меньшего потребления ресурсов и удобного доступа к системным функциям через Rust.
4. Архитектура
Desktop UI (Tauri/React)
↓
Rust System Layer
↓
Windows/macOS Adapters
↓
Yuki Agent Core
↓
Planner + Memory + Context + Permissions
↓
Tool Registry
↓
AI Provider Layer
AI Provider Layer: OpenAI / Claude / Gemini / xAI / OpenRouter / Ollama / LM Studio / Custom API.
5. AI Agent
Agent Core отвечает за понимание задачи, планирование, выбор tools, выполнение, наблюдение, обработку ошибок, memory и permissions.
Agent Loop:
INPUT → UNDERSTAND → PLAN → TOOL SELECTION → EXECUTE → OBSERVE → VERIFY → NEXT STEP / COMPLETE → RESPONSE.
Yuki никогда не должна заявлять об успешном действии без подтверждения tool.
6. Computer Use
Yuki должна уметь:
• запускать/закрывать приложения;
• переключать и управлять окнами;
• нажимать элементы интерфейса;
• вводить текст;
• работать с клавиатурой и мышью;
• читать accessibility/UI tree;
• делать screenshots;
• анализировать экран vision-моделью.
Приоритет — нативные Accessibility/UI APIs; computer vision и mouse simulation — fallback.
7. Browser Agent
Поддержать:
• открытие браузера и URL;
• web search;
• click/type/scroll;
• чтение страниц;
• формы;
• download/upload;
• извлечение данных;
• многошаговые browser tasks.
Рекомендуется Playwright для browser automation.
8. Файлы и документы
Filesystem:
search, read, write, create, rename, move, copy, delete, open, compress, extract.
Поддержать PDF, DOCX, TXT, MD, XLSX, CSV и изображения.
Document Intelligence:
File → Parser/OCR → Text → Chunking → LLM → Answer.
9. Memory
Типы:
• Short-term.
• Session.
• Long-term.
• Episodic.
Можно хранить только разрешённые пользователем данные: имя, язык, предпочтения, рабочие сценарии, любимые приложения, пользовательские команды.
UI: Memory → View / Edit / Delete / Clear All.
По умолчанию память локальная.
10. Голос
Pipeline:
Microphone → VAD → STT → Agent → TTS → Speaker.
Поддержать:
• streaming STT;
• wake word «Yuki» / «Hey Yuki»;
• push-to-talk;
• interruption;
• voice activity detection;
• cloud/local STT и TTS.
11. Личность Yuki
Yuki: умная, спокойная, дружелюбная, слегка игривая, проактивная, краткая и естественная.
Она должна говорить как персональный ассистент, а не как технический chatbot.
12. Avatar
Опциональный VRM/3D avatar.
Состояния: IDLE, LISTENING, THINKING, WORKING, SPEAKING, SUCCESS, ERROR, SLEEPING.
Поддержать lip-sync, blink, facial expressions, emotions, idle animations, always-on-top, прозрачное окно, resize, reposition и click-through.
13. НОВЫЙ главный интерфейс — Orbital Yuki
Главный экран полностью переработать. Не использовать стандартный dashboard с множеством карточек и обычный ChatGPT-подобный layout.
Концепция: премиальный минималистичный «AI command center».
Визуальные принципы:
• dark-first;
• глубокий графитовый фон;
• центральное AI-ядро/Orb;
• мягкое свечение;
• тонкие линии;
• glassmorphism только для вторичных элементов;
• много свободного пространства;
• минимум постоянных панелей;
• плавные micro-interactions;
• motion design;
• фокус на текущем состоянии Yuki.
Центр:
большой интерактивный Orb.
Orb states:
IDLE — медленное дыхание;
LISTENING — реакция на голос;
THINKING — динамические кольца;
WORKING — движение частиц;
SPEAKING — визуальная реакция на TTS;
SUCCESS — короткая позитивная анимация;
ERROR — спокойная индикация ошибки.
Композиция:
верх: YUKI / время / online status;
центр: Orb + короткая фраза «Чем займёмся?»;
низ: единая Command Bar для голоса/текста;
самый низ: компактные Today / Tasks / Next Event;
навигация — минимальная rail, открывающая Chat, Commands, Memory, Activity, Settings.
Главный экран отвечает на вопрос «Что Yuki делает сейчас?», а не «Какие функции есть у приложения?».
14. Design System
Создать единый дизайн-сет:
• современная sans-serif типографика;
• 4/8px spacing grid;
• крупные отступы;
• Orb;
• Command Bar;
• status pills;
• activity chips;
• task timeline;
• confirmation modal;
• settings panels;
• automation nodes.
Motion: 150–250ms обычные transitions, 300–800ms AI-state transitions. Поддержать Reduce Motion.
15. Chat
Chat — вторичный режим, а не главный экран.
Поддержать:
• сообщения;
• voice/text composer;
• attachments;
• markdown;
• code blocks;
• tool status;
• task progress.
Показывать только безопасные статусы: «Ищу файл…», «Открываю браузер…», «Проверяю результат…». Внутренний chain-of-thought не показывать.
16. Commands и Automation
Пользователь может сказать:
«Юки, создай команду “Работа”, которая открывает Chrome, Slack, Notion и VS Code».
Yuki сама создаёт automation.
Triggers: voice phrase, hotkey, schedule, app launched, startup, webhook.
Actions: open app, URL, type, key, mouse, command, clipboard, notification, speak, reminder, message.
Logic: if/else, switch, loop, delay, parallel, wait.
17. Вместо Marketplace — Personal Capability Hub
Публичный Marketplace НЕ нужен.
В приложении должен быть раздел:
Settings → Capabilities
или
Yuki → Extend.
Назначение: единое место для просмотра и добавления возможностей.
Разделы:
• Installed Capabilities;
• Available Integrations;
• Add Tool;
• Add MCP;
• Add API;
• Import Plugin;
• Create Command;
• Repair Capability.
Главная идея: пользователь может попросить Yuki самой добавить способность.
Пример:
«Юки, хочу, чтобы ты управляла Spotify».
Yuki:
1. проверяет существующие tools;
2. ищет подключённый MCP/plugin/integration;
3. предлагает подключить найденное решение;
4. если нужен API key — просит пользователя;
5. если готового решения нет — предлагает создать локальное расширение;
6. выполняет validation;
7. тестирует capability;
8. сообщает результат.
Yuki НЕ должна самостоятельно переписывать свой core без разрешения пользователя.
18. Self-Extension
Ввести Capability Manager.
Capability:
• id;
• name;
• description;
• version;
• permissions;
• tools;
• triggers;
• actions;
• source;
• enabled;
• health status.
Sources:
• built-in;
• MCP;
• local plugin;
• user script;
• API integration.
Yuki может генерировать код расширения, но установка проходит через validation, sandbox и permission review.
19. MCP
Обязательно поддержать Model Context Protocol.
MCP Manager:
• Add Server;
• transport;
• URL/command;
• arguments;
• environment;
• authentication;
• enable/disable;
• permissions;
• Test Connection.
20. Plugin System
Нужен локальный Plugin SDK без публичного marketplace.
Установка:
• local package;
• Git repository;
• development folder;
• Yuki-generated package.
Плагин может добавлять tools, commands, UI panels, AI providers, voice providers, actions и triggers.
Каждый плагин проходит manifest validation и permission review.
21. Permissions
Категории:
• Microphone;
• Screen Recording;
• Accessibility;
• Files;
• Network;
• Shell/Terminal;
• Camera;
• Notifications;
• Browser;
• External services.
macOS: использовать нативные системные разрешения Accessibility, Screen Recording и т.д.
Windows: использовать системные механизмы + собственную permission policy Yuki.
22. Confirmation
Уровни:
LOW — автоматически.
MEDIUM — согласно настройкам.
HIGH — обязательно подтверждение.
HIGH:
• delete;
• shutdown/restart;
• shell;
• system changes;
• financial actions;
• важные сообщения.
Перед опасным действием показать план и Approve / Cancel.
23. Activity Log
Activity → History.
Записывать:
timestamp, tool, target, status, result, duration.
API keys, tokens и другие секреты никогда не записывать в открытом виде.
24. Context Awareness
При наличии разрешений:
• current app;
• active window;
• selected text;
• clipboard;
• screen;
• current file;
• time;
• calendar;
• system status.
Примеры: «Переведи это», «Объясни этот код», «Сохрани это в заметки».
25. Calendar / Reminders / Notifications
Calendar: Google Calendar, Outlook.
Reminders:
• one-time;
• daily;
• weekly;
• custom recurrence.
Notifications:
• Windows;
• macOS;
• sound;
• voice;
• avatar reaction.
26. Web Search
Search → Read Sources → Summarize → Sources.
Yuki должна отличать информацию из памяти от информации, полученной из интернета.
27. Developer Mode
Поддержать Terminal, Git, GitHub, VS Code, Docker, Python и Node.
Shell execution — HIGH risk.
В будущем добавить coding-agent integrations.
28. Remote Control
Не входит в MVP.
Будущее:
Mobile/Web → Secure Gateway → Desktop Yuki.
Поддержать commands, voice/text, status и notifications. Pairing через QR.
29. Privacy / Local First
По умолчанию:
• memory локальная;
• settings локальные;
• activity log локальный;
• credentials в OS secure storage.
Local Only:
• local LLM;
• local STT;
• local TTS;
• local memory;
• без cloud AI.
30. Кроссплатформенный System Adapter
Общие интерфейсы:
SystemAdapter: openApp(), closeApp(), listWindows(), focusWindow(), setVolume(), getSystemInfo().
FileAdapter: search(), read(), write(), move(), copy(), delete().
InputAdapter: keyboard(), mouse().
ScreenAdapter: capture(), accessibilityTree().
WindowsAdapter и MacOSAdapter реализуют интерфейсы отдельно.
31. База данных
Основные таблицы:
users, settings, providers, conversations, messages, memories, tools, permissions, capabilities, plugins, mcp_servers, commands, command_nodes, automations, automation_runs, activity_logs, notifications, tasks.
API keys хранить через OS secure storage.
32. Task Management
Статусы:
QUEUED, RUNNING, WAITING_USER, COMPLETED, FAILED, CANCELLED.
Для долгих задач:
• progress;
• current step;
• cancel;
• retry;
• error recovery.
33. Error Recovery
При ошибке:
1. определить причину;
2. попробовать безопасный fallback;
3. повторить только безопасную/идемпотентную операцию;
4. запросить пользователя при необходимости;
5. сообщить реальную причину.
34. Proactive Assistant
Будущая opt-in функция.
Yuki может предлагать напоминания, подготовку рабочего режима и действия по календарю. Proactive actions всегда ограничены permission policy.
35. System Monitor / Media
System Monitor: CPU, RAM, GPU, Disk, Network, Battery.
Media: system controls, Spotify, YouTube, YouTube Music.
Команды: play, pause, next, previous, volume.
36. Localization
Первый релиз:
• Russian;
• English;
• Ukrainian.
Все UI-строки через i18n layer.
37. Производительность
Цели:
• startup < 3 сек;
• минимальный idle CPU/RAM;
• wake-word latency < 300 мс;
• streaming STT;
• first token желательно < 1.5–2 сек;
• UI 60 FPS.
На обеих ОС поддержать idle/sleep режим.
38. MVP
✓ Windows 10/11
✓ macOS 13+ Intel/Apple Silicon
✓ Tauri + React + TypeScript
✓ Rust system layer
✓ AI Agent
✓ OpenAI / Claude / Gemini
✓ Chat
✓ STT / TTS
✓ Wake word
✓ Yuki personality
✓ Applications
✓ Browser
✓ Files
✓ Keyboard / Mouse
✓ Clipboard
✓ Basic screen understanding
✓ Memory
✓ Reminders
✓ Notifications
✓ Hotkeys
✓ Permissions
✓ Activity log
✓ Capability Hub
✓ MCP
✓ Basic automation
✓ Orbital Interface
39. MVP-2
✓ Advanced computer vision
✓ Visual Command Builder
✓ Calendar
✓ Advanced automation
✓ Ollama
✓ LM Studio
✓ Plugin SDK
✓ Self-extension workflow
✓ VRM avatar
✓ Advanced memory
40. Version 2
✓ Mobile/Web remote control
✓ Telegram
✓ Cloud sync
✓ Proactive assistant
✓ Coding-agent integrations
✓ Advanced avatar/emotions
41. Критерии готовности
На Windows и macOS должны стабильно работать сценарии:
1. «Юки, открой браузер и найди музыку для концентрации».
2. «Найди последний PDF в Downloads и открой его».
3. «Напомни мне завтра в 10:00 позвонить Ивану».
4. «Переведи выделенный текст».
5. «Запусти мой рабочий режим».
6. «Найди файл и перемести его в папку X».
7. «Открой приложение и выполни действие внутри него».
8. «Юки, добавь возможность управлять Spotify» — через существующую integration/MCP/plugin или безопасное создание capability.
42. Ключевой UX-принцип
Пользователь не должен думать категориями «какой tool установить».
Он говорит:
«Юки, сделай X».
Если возможности нет:
«У меня пока нет возможности X. Я могу попробовать добавить её. Разрешить?»
Capability Hub — это внутренний механизм расширения Yuki, а не публичный магазин.
43. Итоговая архитектура проекта
yuki/
├── apps/desktop/
│   ├── src/
│   ├── components/
│   ├── screens/
│   ├── design-system/
│   └── avatar/
├── core/
│   ├── agent/
│   ├── planner/
│   ├── memory/
│   ├── context/
│   ├── permissions/
│   ├── tasks/
│   └── policies/
├── rust/
│   ├── system/
│   ├── windows/
│   ├── macos/
│   ├── filesystem/
│   ├── accessibility/
│   ├── input/
│   └── screen/
├── ai/
│   ├── providers/
│   ├── vision/
│   ├── embeddings/
│   └── local/
├── voice/
│   ├── stt/
│   ├── tts/
│   ├── vad/
│   └── wakeword/
├── tools/
│   ├── browser/
│   ├── filesystem/
│   ├── apps/
│   ├── keyboard/
│   ├── mouse/
│   ├── clipboard/
│   ├── screen/
│   ├── calendar/
│   ├── reminders/
│   └── media/
├── automation/
├── capabilities/
├── plugins/
├── mcp/
├── storage/
└── cloud/
44. Базовая identity-инструкция Yuki
You are Yuki, a personal AI desktop assistant.

Your purpose is to help the user operate their computer, complete tasks, manage information, automate repetitive workflows, and communicate naturally.

When a task requires action:
1. Understand the user's intent.
2. Determine which tools are required.
3. Execute the minimum necessary actions.
4. Verify important results.
5. Report the result clearly.

Never claim an action was completed unless the corresponding tool confirmed successful execution.

For dangerous or irreversible actions, request confirmation according to the permission policy.

Be concise, natural, helpful and slightly playful.

You are Yuki, not ChatGPT.
