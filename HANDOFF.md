> Актуальное продолжение от18сентября: [CLAUDE-HANDOFF.md](CLAUDE-HANDOFF.md). Ниже состояние предыдущего этапа.

# Yuki — продолжение работы

Обновлено: 17 сентября 2026. Рабочая папка D:\github\yuki-astra; старый ai-yuki не менялся. Версия 0.3.0.

## Актуальная выдача

- release/Yuki_0.3.0_x64-setup.exe и release/Yuki_0.3.0_windows-x64-portable.zip, SHA256SUMS-0.3.0.txt.
- Это два Windows-формата, НЕ Windows/macOS. Mac workflow есть, но Mac-сборки нет. Не объявлять всё ТЗ выполненным.
- Полный статус: docs/SPEC-STATUS.md; проверки и замеры: docs/VALIDATION.md; активы/Unity: docs/ANIMATION-IMPORT.md.
- 120 TS/core и 293 Rust теста прошли; production TypeScript, EXE и NSIS собраны.

## Исправлено

- Reminders: атомарный постоянный inbox + in-app карточка + OS toast; окно показывается к сроку, missed-event/restart fallback. Проверен реальный таймер. DST/daily-calendar, звук и голос уведомления ещё не готовы.
- Ошибки/частичный ответ и инструменты сохраняются в истории, статусы раскрываются. UIA COM работает в отдельном потоке, Tauri async не блокирует собственное окно.
- Причиной одного HTTP400 был собственный CLI-adapter, отклонявший screenshot. Реализованы Claude stream-json image blocks и Codex --image временные файлы. Оба CLI реально проверены с синтетическим ORCHID, VISION_OK. Секретные OAuth-файлы не читаем.
- Отдельная страница Telegram, ошибки/статус бота. Настоящую отправку файла в группу НЕ проверяли и root не отправлял сообщений. Bot и Desktop — разные пути.
- Dashboard CPU/GPU/RAM/disks, погода DefaultGeoposition или ручной город (здесь OS location не задан). Команды: поиск/группы/полотно/запуск.
- Главное окно и аватар перетаскиваются: проверено настоящей мышью +100/-30. scripts/check-window-drag.ps1 должен выбирать ТОЛЬКО видимые HWND.
- Удалён avatar/gestures.ts с самодельной хореографией. Готовые VRMA, one-shot/loop/hold, назначение действиям; недоступные sit/dance/walk отключены. Приветствие VRMA_02 начинается стоя и возвращается стоя, без ping-pong.
- Lulum RAM дерево процессов: Working Set 4981→788 MiB, CPU 2.85→1.44% за8сек. Texture rawRGBA603→57MiB, limit1024 diskcache, VRMUtils combineMorphs/combineSkeletons/removeUnnecessaryVertices, FPS cap, один inactive cache60s. Исходники не меняются.
- Miku/Lulum/RadDollShort видны целиком, мимика Lulum проверена после оптимизации. Последний профиль возвращён Lulum.

## Главный оставшийся блокер

Пользователь категорически запретил создавать анимации с нуля. Использовать скачанные authored clips; его новая загрузка — дополнение к 9 замечаниям, НЕ подтверждение активации Unity.

Unity6000.5.4f1 установлен, batch отказал `No valid Unity Editor license found`, exit198. Async-вопрос об активации Personal уже задан, ответа нет. Не обходить лицензию. После ответа запустить tools/motion-converter/BakeMotions (собственный editor script), затем scripts/pack-motions.py. Сам конвертер ещё не смог пройти Unity compile. Детали docs/ANIMATION-IMPORT.md.

6VRM в личной библиотеке: Miku, Lulum, Arisa, AvatarSample_B, RadDoll, RadDollShort. 7готовыхVRMA. Подготовлено144.anim+ArisaFBX, но новые танцы ещё НЕ сконвертированы. 54faceclipShinano/リア-アリス не совпали полностью ни с одной VRM. Scarlet/Rosetta .blend и Resonite пока нерабочие какVRM. Чужиеассеты неупаковывать в публичную поставку.

## Среда / QA

- PowerShell, npm.cmd, Get-Content -Encoding UTF8. Без AGENTS и git remote; не публиковать самовольно.
- Не спавнить subagents: текущая инструкция запрещает без явного запроса.
- skills optimize/frontend-design/agent-browser/openai-docs уже читались в сессии. agent-browser теряет WebView при recreate; fallback scripts/avatar-qa.mjs CDP9223.
- Финальная portable запущена обычным образом: PID31276, путь release/portable-0.3.0/Yuki.exe, без CDP. Тестовые73036/34204/60412 остановлены адресно. Проверено напоминание из трея (hiddenBeforeDue=true, windowVisible=true, banner=true, persisted=true), тестовая запись удалена.
- QA: node scripts/avatar-qa.mjs main|avatar eval tmp/script.js; screenshot tmp/image.png; eval-screenshot tmp/script.js tmp/image.png. DEV yukiDebug вreleaseнет.
- Сначала сохранить/восстановить пользовательские настройки. Не отправлять настоящие Telegram сообщения без явно заданного действия. Снимок/тест CLIvision синтетический.
