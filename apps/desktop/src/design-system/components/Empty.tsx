/**
 * Пустое состояние.
 *
 * Раньше каждый пустой раздел писал одну серую строчку в левом верхнем углу и
 * оставлял остальные девять десятых экрана пустыми — выглядело это как
 * незагрузившаяся страница, а не как «здесь пока ничего нет».
 *
 * Пустое состояние обязано сказать три вещи: что раздел пуст, почему это
 * нормально и что сделать, чтобы он перестал быть пустым. Без третьего пункта
 * человек остаётся в тупике, а это самый частый способ потерять его на
 * первом же экране.
 */

import type { ReactNode } from 'react'

import './Empty.css'

export interface EmptyProps {
  title: string
  body?: string
  /** Кнопка или ссылка: что сделать, чтобы здесь появилось содержимое. */
  action?: ReactNode
  /** Значок раздела — та же графика, что в навигации. */
  icon?: ReactNode
}

export function Empty({ title, body, action, icon }: EmptyProps) {
  return (
    <div className="empty">
      {icon && <div className="empty__icon">{icon}</div>}
      <p className="empty__title">{title}</p>
      {body && <p className="empty__body">{body}</p>}
      {action && <div className="empty__action">{action}</div>}
    </div>
  )
}
