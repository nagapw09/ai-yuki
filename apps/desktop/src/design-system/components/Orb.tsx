import { useMemo } from 'react'
import type { CSSProperties } from 'react'

import type { OrbState } from '../../state/types'
import './Orb.css'

/** Сколько частиц рисуем в состоянии WORKING. */
const PARTICLE_COUNT = 7

/** Подпись состояния для скринридера — визуально состояние передаёт цвет и движение. */
const STATE_LABEL: Record<OrbState, string> = {
  idle: 'Yuki ждёт',
  listening: 'Yuki слушает',
  thinking: 'Yuki думает',
  working: 'Yuki выполняет задачу',
  speaking: 'Yuki отвечает',
  success: 'Задача выполнена',
  error: 'Произошла ошибка',
  sleeping: 'Yuki спит',
}

export interface OrbProps {
  state: OrbState
  /**
   * Амплитуда звука 0…1: громкость микрофона в LISTENING и громкость TTS
   * в SPEAKING. В остальных состояниях игнорируется.
   */
  level?: number
  /** Диаметр в пикселях. */
  size?: number
  onClick?: () => void
}

export function Orb({ state, level = 0, size = 260, onClick }: OrbProps) {
  // Частицы получают разный радиус и фазу, иначе орбита выглядит как одно кольцо.
  const particles = useMemo(
    () =>
      Array.from({ length: PARTICLE_COUNT }, (_, i) => ({
        radius: size * (0.3 + (i % 3) * 0.07),
        duration: 2.6 + (i % 4) * 0.7,
        delay: -(i * 0.42),
      })),
    [size],
  )

  const reactive = state === 'listening' || state === 'speaking'
  const style = {
    '--orb-size': `${size}px`,
    '--orb-color': `var(--orb-${state})`,
    '--orb-level': reactive ? Math.min(1, Math.max(0, level)) : 0,
  } as CSSProperties

  return (
    <button
      type="button"
      className="orb"
      data-state={state}
      style={style}
      onClick={onClick}
      aria-label={STATE_LABEL[state]}
    >
      <span className="orb__layer orb__glow" />

      <svg className="orb__layer orb__rings" viewBox="0 0 100 100" aria-hidden="true">
        <circle
          className="orb__ring orb__ring--outer"
          cx="50"
          cy="50"
          r="46"
          strokeWidth="0.8"
          strokeDasharray="18 10 4 10"
          opacity="0.8"
        />
        <circle
          className="orb__ring orb__ring--inner"
          cx="50"
          cy="50"
          r="38"
          strokeWidth="0.5"
          strokeDasharray="3 8"
          opacity="0.55"
        />
      </svg>

      <span className="orb__layer orb__particles" aria-hidden="true">
        {particles.map((p, i) => (
          <span
            key={i}
            className="orb__particle"
            style={
              {
                '--particle-radius': `${p.radius}px`,
                '--particle-duration': `${p.duration}s`,
                '--particle-delay': `${p.delay}s`,
              } as CSSProperties
            }
          />
        ))}
      </span>

      <span className="orb__layer orb__core" />
    </button>
  )
}
