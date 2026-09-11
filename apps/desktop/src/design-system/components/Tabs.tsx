/**
 * Навигация по разделам.
 *
 * Раньше это была рейка из семи одинаковых иконок без подписей: подпись
 * появлялась только по наведению. Замысел был в том, чтобы не превращать
 * главный экран в перечень возможностей, но на деле человек не мог понять,
 * куда ведёт кнопка, не потыкав в каждую. Подписи вернулись.
 *
 * Иконки остались: они дают форму, за которую цепляется глаз, когда подпись
 * уже прочитана один раз и читать её больше не нужно.
 */

import type { ReactNode } from 'react'

import { useT } from '../../i18n'
import type { ScreenId } from '../../state/types'
import './Tabs.css'

interface Entry {
  id: ScreenId
  labelKey: string
  icon: ReactNode
}

const ENTRIES: Entry[] = [
  { id: 'orbital', labelKey: 'rail.home', icon: <HomeIcon /> },
  { id: 'chat', labelKey: 'rail.chat', icon: <ChatIcon /> },
  { id: 'commands', labelKey: 'rail.commands', icon: <CommandsIcon /> },
  { id: 'memory', labelKey: 'rail.memory', icon: <MemoryIcon /> },
  // Заметок в составе навигации из ТЗ §13 нет: это осознанное дополнение
  // из docs/GAPS.md §5 — без своего места заметки были бы доступны только
  // через просьбу к модели.
  { id: 'notes', labelKey: 'rail.notes', icon: <NotesIcon /> },
  { id: 'activity', labelKey: 'rail.activity', icon: <ActivityIcon /> },
  { id: 'settings', labelKey: 'rail.settings', icon: <SettingsIcon /> },
]

export interface TabsProps {
  current: ScreenId
  onNavigate: (screen: ScreenId) => void
}

export function Tabs({ current, onNavigate }: TabsProps) {
  const t = useT()

  return (
    <nav className="tabs" aria-label={t('rail.label')}>
      {ENTRIES.map((entry) => (
        <button
          key={entry.id}
          type="button"
          className="tabs__item"
          aria-current={current === entry.id ? 'page' : undefined}
          onClick={() => onNavigate(entry.id)}
        >
          {entry.icon}
          <span>{t(entry.labelKey)}</span>
        </button>
      ))}
    </nav>
  )
}

const stroke = {
  stroke: 'currentColor',
  strokeWidth: 1.6,
  strokeLinecap: 'round' as const,
  strokeLinejoin: 'round' as const,
  fill: 'none',
}

function HomeIcon() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="4" {...stroke} />
      <circle cx="12" cy="12" r="9" {...stroke} strokeDasharray="3 5" opacity="0.6" />
    </svg>
  )
}

function ChatIcon() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M4 6.5A2.5 2.5 0 0 1 6.5 4h11A2.5 2.5 0 0 1 20 6.5v7a2.5 2.5 0 0 1-2.5 2.5H9l-5 4z" {...stroke} />
    </svg>
  )
}

function CommandsIcon() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M5 7l4 4-4 4" {...stroke} />
      <path d="M12 16h7" {...stroke} />
    </svg>
  )
}

function MemoryIcon() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M12 4c3 0 5 2 5 4.5 0 1-.4 1.8-1 2.5.6.7 1 1.6 1 2.5C17 16 15 18 12 18s-5-2-5-4.5c0-.9.4-1.8 1-2.5-.6-.7-1-1.5-1-2.5C7 6 9 4 12 4z" {...stroke} />
      <path d="M12 4v14" {...stroke} opacity="0.5" />
    </svg>
  )
}

function NotesIcon() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M6 3h9l4 4v14a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z" {...stroke} />
      <path d="M14 3v5h5" {...stroke} />
      <path d="M9 13h6M9 17h4" {...stroke} opacity="0.7" />
    </svg>
  )
}

function ActivityIcon() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M3 12h4l3-7 4 14 3-7h4" {...stroke} />
    </svg>
  )
}

function SettingsIcon() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="3" {...stroke} />
      <path
        d="M12 3v2m0 14v2M3 12h2m14 0h2M5.6 5.6l1.4 1.4m10 10l1.4 1.4M18.4 5.6L17 7M7 17l-1.4 1.4"
        {...stroke}
      />
    </svg>
  )
}
