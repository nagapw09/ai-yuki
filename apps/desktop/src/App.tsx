import { useCallback, useEffect } from 'react'

import { sendMessage } from './agent/session'
import { isTauri, providerList, systemInfo } from './bridge'
import { ConfirmDialog } from './design-system/components/ConfirmDialog'
import { Rail } from './design-system/components/Rail'
import { useT } from './i18n'
import { Activity } from './screens/Activity'
import { Chat } from './screens/Chat'
import { Memory } from './screens/Memory'
import { Orbital } from './screens/Orbital'
import { Placeholder } from './screens/Placeholder'
import { Settings } from './screens/Settings'
import { useUiStore } from './state/store'
import './App.css'

export function App() {
  const t = useT()
  const screen = useUiStore((s) => s.screen)
  const setScreen = useUiStore((s) => s.setScreen)
  const setOrbState = useUiStore((s) => s.setOrbState)
  const setHeadline = useUiStore((s) => s.setHeadline)
  const orbState = useUiStore((s) => s.orbState)

  useProviderStatus()

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
    // Голосовой конвейер — фаза 2. Пока переключаем только видимое состояние Orb,
    // чтобы состояние LISTENING можно было проверить глазами.
    setOrbState(orbState === 'listening' ? 'idle' : 'listening')
    setHeadline(null)
  }, [orbState, setOrbState, setHeadline])

  return (
    <div className="app">
      <Rail current={screen} onNavigate={setScreen} />
      <div className="app__content">
        {screen === 'orbital' && (
          <Orbital onSubmit={handleSubmit} onToggleVoice={handleToggleVoice} />
        )}
        {screen === 'chat' && <Chat />}
        {screen === 'activity' && <Activity />}
        {screen === 'memory' && <Memory />}
        {screen === 'settings' && <Settings />}
        {screen === 'commands' && <Placeholder title={t('rail.commands')} />}
      </div>
      <ConfirmDialog />
    </div>
  )
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
