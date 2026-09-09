import type { ReactNode } from 'react'

import { useT } from '../../i18n'
import type { ScreenId } from '../../state/types'
import './Rail.css'

interface RailEntry {
  id: ScreenId
  labelKey: string
  icon: ReactNode
}

const ENTRIES: RailEntry[] = [
  { id: 'orbital', labelKey: 'rail.home', icon: <HomeIcon /> },
  { id: 'chat', labelKey: 'rail.chat', icon: <ChatIcon /> },
  { id: 'commands', labelKey: 'rail.commands', icon: <CommandsIcon /> },
  { id: 'memory', labelKey: 'rail.memory', icon: <MemoryIcon /> },
  { id: 'activity', labelKey: 'rail.activity', icon: <ActivityIcon /> },
  { id: 'settings', labelKey: 'rail.settings', icon: <SettingsIcon /> },
]

export interface RailProps {
  current: ScreenId
  onNavigate: (screen: ScreenId) => void
}

export function Rail({ current, onNavigate }: RailProps) {
  const t = useT()

  return (
    <nav className="rail" aria-label={t('rail.label')}>
      {ENTRIES.map((entry, index) => (
        <div key={entry.id} style={{ display: 'contents' }}>
          {/* Настройки отделены от рабочих разделов: это не место, куда ходят
              в процессе работы. */}
          {entry.id === 'settings' && index > 0 && <span className="rail__divider" />}
          <button
            type="button"
            className="rail__item"
            aria-current={current === entry.id ? 'page' : undefined}
            onClick={() => onNavigate(entry.id)}
          >
            {entry.icon}
            <span className="rail__tooltip">{t(entry.labelKey)}</span>
          </button>
        </div>
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
    <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="4" {...stroke} />
      <circle cx="12" cy="12" r="9" {...stroke} strokeDasharray="3 5" opacity="0.6" />
    </svg>
  )
}

function ChatIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M4 6.5A2.5 2.5 0 0 1 6.5 4h11A2.5 2.5 0 0 1 20 6.5v7a2.5 2.5 0 0 1-2.5 2.5H9l-5 4z" {...stroke} />
    </svg>
  )
}

function CommandsIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden="true">
      <rect x="4" y="4" width="6" height="6" rx="2" {...stroke} />
      <rect x="14" y="14" width="6" height="6" rx="2" {...stroke} />
      <path d="M10 7h4a3 3 0 0 1 3 3v4" {...stroke} />
    </svg>
  )
}

function MemoryIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M12 4c3 0 5 2 5 4.5 0 1-.4 1.8-1 2.5.6.7 1 1.6 1 2.5C17 16 15 18 12 18s-5-2-5-4.5c0-.9.4-1.8 1-2.5-.6-.7-1-1.5-1-2.5C7 6 9 4 12 4z" {...stroke} />
      <path d="M12 4v14" {...stroke} opacity="0.5" />
    </svg>
  )
}

function ActivityIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M3 12h4l3-7 4 14 3-7h4" {...stroke} />
    </svg>
  )
}

function SettingsIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="3" {...stroke} />
      <path
        d="M12 3v2m0 14v2M3 12h2m14 0h2M5.6 5.6l1.4 1.4m10 10l1.4 1.4M18.4 5.6L17 7M7 17l-1.4 1.4"
        {...stroke}
      />
    </svg>
  )
}
