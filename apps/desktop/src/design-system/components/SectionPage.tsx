/**
 * Страница с подразделами слева.
 *
 * Настройки были одной лентой из двадцати блоков, и человек видел «кучу
 * настроек в куче». Теперь на экране только выбранный подраздел: три-четыре
 * карточки, которые помещаются в окно без прокрутки на полстраницы.
 *
 * Выбранный подраздел запоминается на время работы приложения: вернувшись в
 * раздел, человек попадает туда, где был, а не в начало.
 */

import { useState, type ReactNode } from 'react'

import './SectionPage.css'

export interface Section {
  id: string
  label: string
  render: () => ReactNode
}

const remembered = new Map<string, string>()

export function SectionPage({ page, sections, initial, footer }: {
  /** Ключ страницы — для памяти о выбранном подразделе. */
  page: string
  sections: Section[]
  /** Подраздел, который открыть явно, например из меню трея. */
  initial?: string
  footer?: ReactNode
}) {
  const [current, setCurrent] = useState(() => initial ?? remembered.get(page) ?? sections[0]!.id)
  const active = sections.find((s) => s.id === current) ?? sections[0]!

  const select = (id: string) => {
    remembered.set(page, id)
    setCurrent(id)
  }

  return (
    <div className="section-page">
      <nav className="section-page__side" aria-label="Подразделы">
        {sections.map((section) => (
          <button
            key={section.id}
            type="button"
            aria-current={section.id === active.id ? 'page' : undefined}
            onClick={() => select(section.id)}
          >
            {section.label}
          </button>
        ))}
        {footer && <div className="section-page__footer">{footer}</div>}
      </nav>
      <div className="section-page__body" key={active.id}>
        <h1 className="section-page__title">{active.label}</h1>
        {active.render()}
      </div>
    </div>
  )
}
