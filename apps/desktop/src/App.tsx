import { listen } from '@tauri-apps/api/event'
import { useCallback, useEffect, useState } from 'react'

import { initTriggers } from './agent/commands'
import { sendMessage } from './agent/session'
import { isVoiceActive, startVoice, stopVoice } from './agent/voice'
import { startAvatarBroadcast } from './avatar/broadcast'
import { startTheme } from './design-system/theme'
import { startVisibility } from './design-system/visibility'
import {
  isTauri,
  NAVIGATE_EVENT,
  onboardingCompleted,
  personaName,
  providerList,
  systemInfo,
} from './bridge'
import { ConfirmDialog } from './design-system/components/ConfirmDialog'
import { Tabs } from './design-system/components/Tabs'
import { TitleBar } from './design-system/components/TitleBar'
import { useT } from './i18n'
import { Activity } from './screens/Activity'
import { Capabilities } from './screens/Capabilities'
import { Chat } from './screens/Chat'
import { Commands } from './screens/Commands'
import { Memory } from './screens/Memory'
import { Onboarding } from './screens/Onboarding'
import { Notes } from './screens/Notes'
import { Orbital } from './screens/Orbital'
import { Settings } from './screens/Settings'
import { useUiStore } from './state/store'
import './App.css'

export function App() {
  const onboarded = useOnboarding()
  const screen = useUiStore((s) => s.screen)
  const setScreen = useUiStore((s) => s.setScreen)
  const setOrbState = useUiStore((s) => s.setOrbState)
  const setHeadline = useUiStore((s) => s.setHeadline)

  useProviderStatus()
  useAssistantName()

  useEffect(() => {
    // Сочетания команд и автозапуск (ТЗ §16). Сбой не должен мешать
    // приложению работать: без триггеров команды остаются доступны вручную.
    if (isTauri()) void initTriggers().catch(() => undefined)
  }, [])

  // Аватар живёт в соседнем окне и состояние знает только отсюда (ТЗ §12).
  useEffect(() => startAvatarBroadcast(), [])

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
      void sendMessage(text)
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
            {screen === 'activity' && <Activity />}
            {screen === 'memory' && <Memory />}
            {screen === 'notes' && <Notes />}
            {screen === 'capabilities' && <Capabilities />}
            {screen === 'settings' && <Settings />}
            {screen === 'commands' && <Commands />}
          </div>
        </>
      ) : (
        <div className="app__content">
          <Onboarding onDone={onboarded.finish} />
        </div>
      )}

      <ConfirmDialog />
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
    personaName()
      .then((name) => {
        if (!cancelled) setAssistantName(name)
      })
      .catch(() => undefined)

    return () => {
      cancelled = true
    }
  }, [setAssistantName, screen])
}

/**
 * Показывает в статусе, настроен ли провайдер (ТЗ §13).
 *
 * До настройки Yuki не может выполнить ни одной задачи, поэтому «не настроен
 * провайдер» — это не украшение, а единственная честная подпись.
 */
function useProviderStatus() {
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
  }, [setConnection, screen])
}
