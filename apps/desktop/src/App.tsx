import { listen } from '@tauri-apps/api/event'
import { useCallback, useEffect, useState } from 'react'

import { initTriggers } from './agent/commands'
import { sendMessage } from './agent/session'
import { isVoiceActive, startVoice, stopVoice } from './agent/voice'
import { startAvatarBroadcast } from './avatar/broadcast'
import { startRemote } from './agent/remote'
import { startTheme } from './design-system/theme'
import { startVisibility } from './design-system/visibility'
import {
  isTauri,
  NAVIGATE_EVENT,
  onboardingCompleted,
  personaName,
  providerList,
  settingGet,
  systemInfo,
} from './bridge'
import { ConfirmDialog } from './design-system/components/ConfirmDialog'
import { ReminderNotice } from './design-system/components/ReminderNotice'
import { Tabs } from './design-system/components/Tabs'
import { TitleBar } from './design-system/components/TitleBar'
import { useT } from './i18n'
import { Activity } from './screens/Activity'
import { Capabilities } from './screens/Capabilities'
import { Chat } from './screens/Chat'
import { Companion } from './screens/Companion'
import { Commands } from './screens/Commands'
import { Memory } from './screens/Memory'
import { Onboarding } from './screens/Onboarding'
import { Notes } from './screens/Notes'
import { Orbital } from './screens/Orbital'
import { Settings, Remote } from './screens/Settings'
import { useUiStore } from './state/store'
import './App.css'

export function App() {
  const onboarded = useOnboarding()
  const screen = useUiStore((s) => s.screen)
  const setScreen = useUiStore((s) => s.setScreen)
  const setOrbState = useUiStore((s) => s.setOrbState)
  const setHeadline = useUiStore((s) => s.setHeadline)

  useProviderStatus(onboarded?.done ?? false)
  useAssistantName()

  useEffect(() => {
    // Сочетания команд и автозапуск (ТЗ §16). Сбой не должен мешать
    // приложению работать: без триггеров команды остаются доступны вручную.
    if (isTauri()) void initTriggers().catch(() => undefined)
  }, [])

  // Постоянное прослушивание имени (ТЗ §37).
  //
  // Только если человек включил его явно: микрофон, который открывается сам по
  // себе при запуске, — это не удобство, а сюрприз. Онбординг ждать не нужно,
  // но до его завершения настройки ещё нет, и условие просто не выполнится.
  useEffect(() => {
    if (!isTauri()) return
    let cancelled = false

    void settingGet('voice.wake.always')
      .then((value) => {
        if (cancelled || value !== 'on' || isVoiceActive()) return
        return startVoice('wake_word')
      })
      .catch((error: unknown) => {
        // Не настроено распознавание или занят микрофон: молчать нельзя —
        // человек ждёт, что его услышат, — но и мешать запуску незачем.
        setHeadline(typeof error === 'string' ? error : 'Не удалось начать слушать')
      })

    return () => {
      cancelled = true
    }
  }, [setHeadline])

  // Аватар живёт в соседнем окне и состояние знает только отсюда (ТЗ §12).
  useEffect(() => startAvatarBroadcast(), [])

  // Просьбы с телефона выполняет этот же цикл (ТЗ §28): Rust только принимает
  // сообщение и проверяет, что чат сопряжён.
  useEffect(() => startRemote(), [])

  // Тема применяется до первого кадра и следит за системной (docs/GAPS.md §8).
  useEffect(() => startTheme(), [])

  // Спрятанное в трей окно не должно продолжать анимировать: это
  // измеренные 21 % одного ядра впустую.
  useEffect(() => startVisibility(), [])

  // Переходы из меню трея: меню живёт в Rust и о экранах не знает.
  useEffect(() => {
    if (!isTauri()) return

    const pending = listen<string>(NAVIGATE_EVENT, (event) => {
      setScreen(event.payload as Parameters<typeof setScreen>[0])
    })

    return () => {
      void pending.then((unlisten) => unlisten())
    }
  }, [setScreen])

  const handleSubmit = useCallback(
    (text: string) => {
      // Разговор ведётся в чате, а Orbital остаётся экраном состояния (ТЗ §13):
      // как только появляется что обсуждать, переключаемся туда.
      setScreen('chat')
      void sendMessage(text).catch(() => undefined)
    },
    [setScreen],
  )

  const handleToggleVoice = useCallback(() => {
    setHeadline(null)
    if (isVoiceActive()) {
      void stopVoice()
      return
    }
    // Push-to-talk по умолчанию: постоянное прослушивание включается
    // отдельно в настройках — это осознанное решение, а не побочный эффект
    // нажатия на микрофон.
    void startVoice('push_to_talk').catch(() => undefined)
  }, [setHeadline])

  // Строка заголовка рисуется всегда, даже пока мы не знаем, куда идти:
  // у окна нет системной рамки, и без неё это единственный способ его
  // передвинуть или закрыть. Пропустить её на время загрузки значило бы
  // запереть человека в окне, которое ничем не управляется.
  return (
    <div className="app">
      <div className="app__glow" aria-hidden="true" />
      <TitleBar />

      {onboarded === null ? (
        <div className="app__content" />
      ) : onboarded.done ? (
        <>
          <Tabs current={screen} onNavigate={setScreen} />
          <div className="app__content">
            {screen === 'orbital' && (
              <Orbital onSubmit={handleSubmit} onToggleVoice={handleToggleVoice} />
            )}
            {screen === 'chat' && <Chat />}
            {screen === 'companion' && <Companion />}
            {screen === 'activity' && <Activity />}
            {screen === 'memory' && <Memory />}
            {screen === 'notes' && <Notes />}
            {screen === 'capabilities' && <Capabilities />}
            {screen === 'settings' && <Settings />}
            {screen === 'commands' && <Commands />}
            {screen === 'telegram' && <div className="settings"><div className="settings__inner"><header><h1>Telegram</h1><p>Управление Yuki с телефона и состояние подключения.</p></header><Remote /><section className="settings__section"><h2>Telegram на компьютере</h2><p>Работа с вашим аккаунтом выполняется через установленный Telegram Desktop. Разрешите чтение интерфейса, управление вводом и доступ к файлам в настройках.</p><button className="settings__button" onClick={()=>handleSubmit('Найди открытое окно Telegram Desktop и прочитай его интерфейс. Ничего не отправляй.')}>Проверить доступ к Telegram Desktop</button></section></div></div>}
          </div>
        </>
      ) : (
        <div className="app__content">
          <Onboarding onDone={onboarded.finish} />
        </div>
      )}

      <ConfirmDialog />
      <ReminderNotice />
    </div>
  )
}

/**
 * Пройден ли мастер первого запуска (docs/GAPS.md §1).
 *
 * В браузере мастера нет: он спрашивает про системные разрешения, которых
 * вне приложения не существует.
 */
function useOnboarding(): { done: boolean; finish: () => void } | null {
  const [done, setDone] = useState<boolean | null>(null)

  useEffect(() => {
    if (!isTauri()) {
      setDone(true)
      return
    }

    let cancelled = false
    onboardingCompleted()
      .then((value) => {
        if (!cancelled) setDone(value)
      })
      // Сбой чтения настройки не должен запереть человека в мастере навсегда.
      .catch(() => {
        if (!cancelled) setDone(true)
      })

    return () => {
      cancelled = true
    }
  }, [])

  if (done === null) return null
  return { done, finish: () => setDone(true) }
}

/**
 * Читает имя ассистента.
 *
 * Один раз при запуске и снова при возврате из настроек: переименование
 * должно быть видно сразу, а следить за настройкой событием ради строки,
 * которая меняется раз в полгода, не стоит.
 */
function useAssistantName() {
  const setAssistantName = useUiStore((s) => s.setAssistantName)
  const screen = useUiStore((s) => s.screen)

  useEffect(() => {
    if (!isTauri()) return

    let cancelled = false
    const changed = listen('yuki://character-changed', () => { void personaName().then(name => { if (!cancelled) setAssistantName(name) }) })
    personaName()
      .then((name) => {
        if (!cancelled) setAssistantName(name)
      })
      .catch(() => undefined)

    return () => {
      cancelled = true
      void changed.then(off => off())
    }
  }, [setAssistantName, screen])
}

/**
 * Показывает в статусе, настроен ли провайдер (ТЗ §13).
 *
 * До настройки Yuki не может выполнить ни одной задачи, поэтому «не настроен
 * провайдер» — это не украшение, а единственная честная подпись.
 */
function useProviderStatus(onboarded: boolean) {
  const setConnection = useUiStore((s) => s.setConnection)
  const screen = useUiStore((s) => s.screen)

  useEffect(() => {
    if (!isTauri()) {
      setConnection({ online: false, providerLabel: '' })
      return
    }

    let cancelled = false

    void (async () => {
      try {
        // Заодно проверяем, что мост до Rust-слоя вообще работает.
        await systemInfo()
        const providers = await providerList()
        const active = providers.find((p) => p.isDefault && p.enabled)
        if (cancelled) return
        setConnection({
          online: Boolean(active),
          providerLabel: active?.label ?? '',
        })
      } catch {
        if (!cancelled) setConnection({ online: false, providerLabel: '' })
      }
    })()

    return () => {
      cancelled = true
    }
    // Перечитываем при возврате с настроек: пользователь мог только что
    // ввести ключ, и статус должен это отразить без перезапуска.
  }, [setConnection, screen, onboarded])
}
