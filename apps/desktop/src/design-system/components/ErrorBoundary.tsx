import { Component } from 'react'
import type { ErrorInfo, ReactNode } from 'react'

interface State {
  error: Error | null
}

/**
 * Показывает ошибку рендера вместо пустого окна.
 *
 * Без этого любое исключение в дереве React оставляет пользователя наедине с
 * чёрным прямоугольником: WebView не показывает ни консоли, ни сообщения, и
 * отличить «приложение сломалось» от «приложение думает» невозможно. Стек
 * оставлен на экране намеренно — его нужно уметь скопировать в отчёт об ошибке.
 */
export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  override state: State = { error: null }

  static getDerivedStateFromError(error: Error): State {
    return { error }
  }

  override componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('Сбой рендера Yuki', error, info.componentStack)
  }

  override render() {
    const { error } = this.state
    if (!error) return this.props.children

    return (
      <div
        style={{
          display: 'grid',
          alignContent: 'center',
          gap: 'var(--space-4)',
          height: '100%',
          padding: 'var(--space-8)',
        }}
      >
        <h1 style={{ margin: 0, fontSize: 'var(--text-lg)', fontWeight: 'var(--weight-medium)' }}>
          Yuki не смогла отрисовать интерфейс
        </h1>
        <pre
          data-selectable
          style={{
            margin: 0,
            padding: 'var(--space-4)',
            maxHeight: '50vh',
            overflow: 'auto',
            background: 'var(--surface-raised)',
            border: '1px solid var(--line-default)',
            borderRadius: 'var(--radius-md)',
            color: 'var(--accent-error)',
            font: 'var(--text-xs) / var(--leading-relaxed) var(--font-mono)',
            whiteSpace: 'pre-wrap',
          }}
        >
          {error.stack ?? error.message}
        </pre>
      </div>
    )
  }
}
