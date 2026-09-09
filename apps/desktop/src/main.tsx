import { getCurrentWindow } from '@tauri-apps/api/window'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import { App } from './App'
import { AvatarWindow } from './avatar/AvatarWindow'
import { ErrorBoundary } from './design-system/components/ErrorBoundary'
import { I18nProvider } from './i18n'
import './design-system/tokens.css'
import './design-system/base.css'

const root = document.getElementById('root')
if (!root) throw new Error('Корневой элемент #root не найден')

/**
 * Оба окна загружают одну и ту же страницу, и различаются они меткой.
 *
 * Вторая точка входа означала бы второй html и вторую сборку — ради
 * одного условия это не окупается.
 */
const isAvatar = (() => {
  try {
    return getCurrentWindow().label === 'avatar'
  } catch {
    // Вне Tauri (например в браузере при разработке) окно всегда главное.
    return false
  }
})()

// Страница у обоих окон одна, а фон у них разный: окну аватара нужен
// прозрачный, иначе поверх всего висит тёмный прямоугольник.
if (isAvatar) {
  document.documentElement.dataset.window = 'avatar'
}

createRoot(root).render(
  <StrictMode>
    <ErrorBoundary>
      {isAvatar ? (
        <AvatarWindow />
      ) : (
        <I18nProvider>
          <App />
        </I18nProvider>
      )}
    </ErrorBoundary>
  </StrictMode>,
)
