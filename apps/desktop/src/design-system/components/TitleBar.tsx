/**
 * Строка заголовка (ТЗ §13).
 *
 * Системная рамка Windows рисуется цветом акцента системы и не знает ничего
 * о теме приложения: поверх графитового интерфейса висела синяя полоса, и
 * первое, что видел человек, было чужим. Поэтому окно без украшений, а
 * заголовок свой — вместе с перетаскиванием и кнопками окна, которые вместе с
 * рамкой пропали бы совсем.
 *
 * Здесь же живут часы и состояние провайдера: раньше они стояли на главном
 * экране и исчезали при переходе в любой раздел, хотя отвечают на вопросы,
 * которые не зависят от раздела.
 */

import { getCurrentWindow } from '@tauri-apps/api/window'
import { useCallback, useEffect, useState } from 'react'

import { isTauri } from '../../bridge'
import { useI18n, useT } from '../../i18n'
import { useUiStore } from '../../state/store'
import './TitleBar.css'

export function TitleBar() {
  const t = useT()
  const { locale } = useI18n()
  const connection = useUiStore((s) => s.connection)
  const setScreen = useUiStore((s) => s.setScreen)
  const name = useUiStore((s) => s.assistantName)
  const now = useClock()

  const time = new Intl.DateTimeFormat(locale, {
    hour: '2-digit',
    minute: '2-digit',
  }).format(now)

  return (
    <header className="titlebar" data-tauri-drag-region>
      <span className="titlebar__mark" data-tauri-drag-region>
        {name || t('app.name')}
      </span>

      <span className="titlebar__spacer" data-tauri-drag-region />

      <time className="titlebar__clock" data-tauri-drag-region>
        {time}
      </time>

      {/* Состояние провайдера — кнопка, а не надпись: единственное разумное
          действие при «не настроен» это пойти и настроить. */}
      <button
        type="button"
        className="titlebar__status"
        data-online={connection.online || undefined}
        onClick={() => setScreen('settings')}
      >
        <span className="titlebar__dot" />
        {connection.online ? connection.providerLabel : t('status.offline')}
      </button>

      <WindowButtons />
    </header>
  )
}

function WindowButtons() {
  const t = useT()
  const [maximized, setMaximized] = useState(false)

  useEffect(() => {
    if (!isTauri()) return

    const window = getCurrentWindow()
    void window.isMaximized().then(setMaximized).catch(() => undefined)

    const pending = window.onResized(() => {
      void window.isMaximized().then(setMaximized).catch(() => undefined)
    })

    return () => {
      void pending.then((unlisten) => unlisten())
    }
  }, [])

  const act = useCallback((run: (w: ReturnType<typeof getCurrentWindow>) => Promise<unknown>) => {
    if (!isTauri()) return
    void run(getCurrentWindow()).catch(() => undefined)
  }, [])

  return (
    <div className="titlebar__buttons">
      <button
        type="button"
        className="titlebar__button"
        aria-label={t('window.minimize')}
        onClick={() => act((w) => w.minimize())}
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <path d="M0 5h10" stroke="currentColor" strokeWidth="1" />
        </svg>
      </button>

      <button
        type="button"
        className="titlebar__button"
        aria-label={t(maximized ? 'window.restore' : 'window.maximize')}
        onClick={() => act((w) => w.toggleMaximize())}
      >
        {maximized ? (
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <rect x="0.5" y="2.5" width="7" height="7" stroke="currentColor" fill="none" />
            <path d="M2.5 2.5V0.5h7v7h-2" stroke="currentColor" fill="none" />
          </svg>
        ) : (
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <rect x="0.5" y="0.5" width="9" height="9" stroke="currentColor" fill="none" />
          </svg>
        )}
      </button>

      {/* Крестик прячет окно в трей, а не закрывает приложение: так решено
          в tray.rs, и подпись не должна обещать другого. */}
      <button
        type="button"
        className="titlebar__button titlebar__button--close"
        aria-label={t('window.hide')}
        onClick={() => act((w) => w.close())}
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <path d="M0 0l10 10M10 0L0 10" stroke="currentColor" strokeWidth="1" />
        </svg>
      </button>
    </div>
  )
}

/**
 * Часы, обновляющиеся на границе минуты.
 *
 * Тикать раз в секунду ради часов и минут — значит будить рендер шестьдесят
 * раз впустую, а бюджет простоя задан в ТЗ §37.
 */
function useClock(): Date {
  const [now, setNow] = useState(() => new Date())

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>

    const schedule = () => {
      timer = setTimeout(() => {
        setNow(new Date())
        schedule()
      }, 60_000 - (Date.now() % 60_000))
    }

    schedule()
    return () => clearTimeout(timer)
  }, [])

  return now
}
