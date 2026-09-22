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
import { useCallback, useEffect, useState, type ReactNode } from 'react'

import { isTauri } from '../../bridge'
import { useT } from '../../i18n'
import { useUiStore } from '../../state/store'
import './TitleBar.css'

export function TitleBar({ children }: { children?: ReactNode }) {
  const t = useT()
  const name = useUiStore((s) => s.assistantName)

  // Навигация живёт в строке заголовка, как у Astra: отдельная строка вкладок
  // съедала 44 px высоты окна, которому и так мало места.
  return (
    <header className="titlebar" onPointerDown={event=>{if(event.button===0&&!(event.target as HTMLElement).closest('button')&&isTauri())void getCurrentWindow().startDragging().catch(console.warn)}} onDoubleClick={event=>{if(!(event.target as HTMLElement).closest('button')&&isTauri())void getCurrentWindow().toggleMaximize().catch(console.warn)}}>
      <span className="titlebar__mark">
        <svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M12 2l2.2 7.8L22 12l-7.8 2.2L12 22l-2.2-7.8L2 12l7.8-2.2z" /></svg>
        {name || t('app.name')}
      </span>
      <div className="titlebar__nav">{children}</div>
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
