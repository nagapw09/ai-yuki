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
  // Состав и порядок — как у Astra. Чат открывается кнопкой с главной, память
  // и заметки живут в разделе «ИИ», журнал и Telegram — в настройках.
  { id: 'orbital', labelKey: 'Главная', icon: <HomeIcon /> },
  { id: 'commands', labelKey: 'Команды', icon: <CommandsIcon /> },
  { id: 'ai', labelKey: 'ИИ', icon: <ChipIcon /> },
  { id: 'voice', labelKey: 'Голос', icon: <MicIcon /> },
  { id: 'companion', labelKey: 'Персонаж', icon: <PersonIcon /> },
  { id: 'settings', labelKey: 'Настройки', icon: <SettingsIcon /> },
]

/** Какой пункт подсветить: вложенные экраны принадлежат своему разделу. */
const OWNER: Partial<Record<ScreenId, ScreenId>> = {
  chat: 'orbital',
  memory: 'ai',
  notes: 'ai',
  activity: 'settings',
  telegram: 'settings',
  capabilities: 'settings',
}

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
          title={t(entry.labelKey)}
          aria-label={t(entry.labelKey)}
          aria-current={(OWNER[current] ?? current) === entry.id ? 'page' : undefined}
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
    <svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M4 11l8-7 8 7v9h-5v-6H9v6H4z" {...stroke} />
    </svg>
  )
}

function ChipIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true">
      <rect x="6" y="6" width="12" height="12" rx="2" {...stroke} />
      <path d="M9 2v4M15 2v4M9 18v4M15 18v4M2 9h4M2 15h4M18 9h4M18 15h4" {...stroke} />
    </svg>
  )
}

function MicIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true">
      <rect x="9" y="3" width="6" height="11" rx="3" {...stroke} />
      <path d="M5 11a7 7 0 0 0 14 0M12 18v3" {...stroke} />
    </svg>
  )
}

function PersonIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="7" r="3.5" {...stroke} />
      <path d="M5 21c.8-4 3.6-6.5 7-6.5s6.2 2.5 7 6.5" {...stroke} />
    </svg>
  )
}


function CommandsIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M5 7l4 4-4 4" {...stroke} />
      <path d="M12 16h7" {...stroke} />
    </svg>
  )
}




function SettingsIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="3" {...stroke} />
      <path
        d="M12 3v2m0 14v2M3 12h2m14 0h2M5.6 5.6l1.4 1.4m10 10l1.4 1.4M18.4 5.6L17 7M7 17l-1.4 1.4"
        {...stroke}
      />
    </svg>
  )
}
