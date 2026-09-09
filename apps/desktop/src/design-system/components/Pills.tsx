import type { ReactNode } from 'react'

import './Pills.css'

export type StatusTone = 'online' | 'offline' | 'warning' | 'error'

export interface StatusPillProps {
  tone: StatusTone
  children: ReactNode
}

export function StatusPill({ tone, children }: StatusPillProps) {
  return (
    <span className="status-pill" data-tone={tone}>
      <span className="status-pill__dot" aria-hidden="true" />
      {children}
    </span>
  )
}

export interface ActivityChipProps {
  label: string
  value: ReactNode
  /** Значения нет — chip остаётся на месте, но гасится: полоса не должна прыгать. */
  empty?: boolean
}

export function ActivityChip({ label, value, empty = false }: ActivityChipProps) {
  return (
    <span className="activity-chip" data-empty={empty}>
      <span className="activity-chip__label">{label}</span>
      <span className="activity-chip__value">{value}</span>
    </span>
  )
}
