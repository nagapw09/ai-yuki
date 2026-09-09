import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import { App } from './App'
import { ErrorBoundary } from './design-system/components/ErrorBoundary'
import { I18nProvider } from './i18n'
import './design-system/tokens.css'
import './design-system/base.css'

const root = document.getElementById('root')
if (!root) throw new Error('Корневой элемент #root не найден')

createRoot(root).render(
  <StrictMode>
    <ErrorBoundary>
      <I18nProvider>
        <App />
      </I18nProvider>
    </ErrorBoundary>
  </StrictMode>,
)
