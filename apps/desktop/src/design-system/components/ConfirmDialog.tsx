import { useEffect, useRef } from 'react'

import { useChatStore } from '../../state/chatStore'
import './ConfirmDialog.css'

/**
 * Подтверждение опасного действия (ТЗ §22).
 *
 * Показывает план — что именно и с чем будет сделано, — а не абстрактное
 * «выполнить действие?». ТЗ §22 требует именно показа плана: подтверждение,
 * из которого не видно, что произойдёт, ничего не подтверждает.
 *
 * Опасная кнопка не выбрана по умолчанию: фокус стоит на отмене, Escape и клик
 * по фону тоже отменяют. Случайный Enter не должен ничего удалять.
 */
export function ConfirmDialog() {
  const confirmation = useChatStore((s) => s.confirmation)
  const resolve = useChatStore((s) => s.resolveConfirmation)
  const cancelRef = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    if (!confirmation) return
    cancelRef.current?.focus()

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        resolve(false)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [confirmation, resolve])

  if (!confirmation) return null

  return (
    <div
      className="confirm__backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) resolve(false)
      }}
    >
      <div
        className="confirm"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
      >
        <span className="confirm__risk" data-risk={confirmation.risk}>
          {confirmation.risk === 'high' ? 'Опасное действие' : 'Требует подтверждения'}
        </span>

        <h2 className="confirm__title" id="confirm-title">
          Разрешить это действие?
        </h2>

        <pre className="confirm__plan" data-selectable>
          {confirmation.plan}
        </pre>

        <div className="confirm__actions">
          <button
            ref={cancelRef}
            type="button"
            className="confirm__button"
            onClick={() => resolve(false)}
          >
            Отмена
          </button>
          <button
            type="button"
            className="confirm__button confirm__button--approve"
            data-risk={confirmation.risk}
            onClick={() => resolve(true)}
          >
            Разрешить
          </button>
        </div>
      </div>
    </div>
  )
}
