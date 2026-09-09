import { useCallback, useEffect } from 'react'

import { isTauri, settingGet, systemInfo } from './bridge'
import { Rail } from './design-system/components/Rail'
import { useT } from './i18n'
import { Orbital } from './screens/Orbital'
import { Placeholder } from './screens/Placeholder'
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
      // Agent Loop (ТЗ §5) появляется в фазе 1. До этого Yuki честно показывает,
      // что приняла команду, и не изображает выполнение: инвариант «не рапортовать
      // об успехе без подтверждения инструмента» действует и на заглушку.
      setOrbState('thinking')
      setHeadline(text)
    },
    [setOrbState, setHeadline],
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
        {screen === 'orbital' ? (
          <Orbital onSubmit={handleSubmit} onToggleVoice={handleToggleVoice} />
        ) : (
          <Placeholder title={t(`rail.${screen}`)} />
        )}
      </div>
    </div>
  )
}

/**
 * Определяет, настроен ли AI-провайдер, и показывает это в статусе (ТЗ §13).
 *
 * До настройки провайдера Yuki не может выполнить ни одной задачи, поэтому статус
 * «не настроен провайдер» — это не украшение, а единственная честная подпись.
 */
function useProviderStatus() {
  const setConnection = useUiStore((s) => s.setConnection)

  useEffect(() => {
    if (!isTauri()) {
      setConnection({ online: false, providerLabel: '' })
      return
    }

    let cancelled = false

    void (async () => {
      try {
        const provider = await settingGet('provider.default')
        // Системную информацию запрашиваем заодно: это заодно проверка того,
        // что мост до Rust-слоя вообще работает.
        await systemInfo()
        if (cancelled) return
        setConnection({ online: Boolean(provider), providerLabel: provider ?? '' })
      } catch {
        if (!cancelled) setConnection({ online: false, providerLabel: '' })
      }
    })()

    return () => {
      cancelled = true
    }
  }, [setConnection])
}
