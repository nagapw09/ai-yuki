<h1 align="center">Yuki</h1>

<p align="center">Персональный AI-компаньон, который живёт на твоём компьютере.<br/>
Голос и текст → понимание → план → инструменты → действие → проверка → результат.</p>

<p align="center">
  <img alt="platform" src="https://img.shields.io/badge/Windows_10%2F11-supported-2b6cb0">
  <img alt="platform" src="https://img.shields.io/badge/macOS_13%2B-Intel_%7C_Apple_Silicon-2b6cb0">
  <img alt="stack" src="https://img.shields.io/badge/Tauri_2-Rust_%2B_React-fa8231">
  <img alt="license" src="https://img.shields.io/badge/license-MIT-555">
</p>

---

## Что это

Yuki разговаривает естественным языком, управляет приложениями, браузером, файлами и
системой, выполняет многошаговые задачи, помнит предпочтения и создаёт автоматизации.

Ключевой принцип UX: пользователь не думает категориями «какой инструмент установить».
Он говорит «Юки, сделай X». Если возможности нет — Yuki отвечает:

> «У меня пока нет возможности X. Я могу попробовать добавить её. Разрешить?»

Публичного маркетплейса нет — есть **Personal Capability Hub**, внутренний механизм
расширения (ТЗ §17).

## Главный экран — Orbital

Не дашборд с карточками и не клон ChatGPT. Тёмный премиальный «AI command center»:
живой **Orb** в центре, единая **Command Bar** снизу, минимальная навигационная рейка.
Экран отвечает на вопрос «что Yuki делает **сейчас**», а не «какие есть функции».

Состояния Orb: `IDLE` · `LISTENING` · `THINKING` · `WORKING` · `SPEAKING` · `SUCCESS` · `ERROR`

## Стек

- **Desktop** — Rust + Tauri 2.x
- **UI** — React + TypeScript + Vite
- **Agent Core** — TypeScript
- **System Layer** — Rust, через `SystemAdapter` → `WindowsAdapter` / `MacOSAdapter`
- **Storage** — SQLite + локальный vector layer; секреты в OS secure storage
- **AI** — OpenAI / Claude / Gemini / xAI / OpenRouter / Ollama / LM Studio / Custom API

Python в ядре **не используется** — почему, разобрано в [ADR-001](docs/STACK-DECISION.md).
Он остаётся опциональным рантаймом для плагинов.

## Приватность

Local-first по умолчанию: память, настройки и журнал активности лежат локально,
ключи — в Keychain (macOS) / Credential Manager (Windows). Есть режим **Local Only** —
локальные LLM, STT, TTS и память, без облачного AI вообще.

## Быстрый старт

Нужны Node ≥ 20, Rust stable и системные зависимости Tauri 2
([инструкция](https://v2.tauri.app/start/prerequisites/)).

```bash
git clone https://github.com/nagapw09/ai-yuki.git
cd ai-yuki
npm install
npm run tauri:dev
```

Сборка инсталлятора под текущую ОС:

```bash
npm run tauri:build
```

## Структура

```
apps/desktop/     UI (React) + Tauri shell (src-tauri)
core/             Agent · Planner · Memory · Context · Permissions · Tasks · Policies
rust/             System layer: system · windows · macos · filesystem
                  accessibility · input · screen
ai/               Провайдеры · vision · embeddings · local
voice/            STT · TTS · VAD · wake word
tools/            browser · filesystem · apps · keyboard · mouse · clipboard
                  screen · calendar · reminders · media
automation/       Триггеры, действия, логика
capabilities/     Capability Manager и self-extension
plugins/          Локальный Plugin SDK
mcp/              Model Context Protocol
storage/          Схема БД и доступ к данным
```

## Документация

- [Техническое задание](docs/TZ.md) — источник истины
- [Архитектура](docs/ARCHITECTURE.md)
- [ADR-001: выбор стека](docs/STACK-DECISION.md)
- [Роадмап](docs/ROADMAP.md)

## Статус

Фаза 0 — каркас. Актуальный прогресс по фазам см. в [роадмапе](docs/ROADMAP.md).

## Лицензия

[MIT](LICENSE)
