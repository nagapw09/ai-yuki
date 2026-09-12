/**
 * Главный экран (ТЗ §13).
 *
 * # Почему он выглядит как сводка, а не как пустота с шаром
 *
 * Раньше в центре был Orb, под ним строка ввода, а внизу четыре мелких чипа —
 * и всё. Экран отвечал на вопрос «что Yuki делает сейчас» ровно одним словом
 * и не отвечал больше ни на что; девять десятых площади оставались пустыми.
 *
 * Теперь вокруг центра живут короткие сводки: погода, память, задачи,
 * ближайшее событие. Каждая — настоящие данные, а не заполнение: виджет,
 * которому нечего сказать, не показывается вовсе, а не висит с прочерком.
 */

import { useEffect, useMemo, useState, type ReactNode } from 'react'

import {
  isTauri,
  settingGet,
  systemInfo,
  weatherGet,
  type SystemInfo,
  type Weather,
} from '../bridge'
import { CommandBar } from '../design-system/components/CommandBar'
import { Orb } from '../design-system/components/Orb'
import { useI18n, useT } from '../i18n'
import { activeTasks, useUiStore } from '../state/store'
import type { OrbState } from '../state/types'
import { cityFromTimeZone, localTimeZone } from './city'
import './Orbital.css'

/** Реплика под Orb для состояний, у которых она предопределена (ТЗ §13). */
const HEADLINE_KEY: Partial<Record<OrbState, string>> = {
  listening: 'orbital.listening',
  thinking: 'orbital.thinking',
  working: 'orbital.working',
  speaking: 'orbital.speaking',
  sleeping: 'orbital.sleeping',
}

export interface OrbitalProps {
  onSubmit: (text: string) => void
  onToggleVoice: () => void
}

export function Orbital({ onSubmit, onToggleVoice }: OrbitalProps) {
  const t = useT()
  const { locale } = useI18n()

  const orbState = useUiStore((s) => s.orbState)
  const audioLevel = useUiStore((s) => s.audioLevel)
  const headline = useUiStore((s) => s.headline)
  const nextEvent = useUiStore((s) => s.nextEvent)
  const tasks = useUiStore((s) => s.tasks)
  const setScreen = useUiStore((s) => s.setScreen)
  const name = useUiStore((s) => s.assistantName)

  const running = useMemo(() => activeTasks(tasks), [tasks])

  const dateLabel = useMemo(
    () =>
      new Intl.DateTimeFormat(locale, {
        weekday: 'long',
        day: 'numeric',
        month: 'long',
      }).format(new Date()),
    [locale],
  )

  // Явно заданная реплика важнее шаблонной: во время задачи Yuki говорит,
  // что именно она делает («Ищу файл…»), а не общее «Работаю…». В покое
  // говорить нечего, и вместо реплики стоит подпись под именем.
  const key = HEADLINE_KEY[orbState]
  const phrase = headline ?? (key ? t(key) : null)

  return (
    <div className="orbital">
      <div className="orbital__side orbital__side--left">
        <WeatherCard />
        <Card label={t('orbital.today')} value={dateLabel} />
      </div>

      <div className="orbital__side orbital__side--right">
        <MemoryCard />
        <Card
          label={t('orbital.tasks')}
          value={
            running.length > 0
              ? t('orbital.tasksCount', { count: running.length })
              : t('orbital.noTasks')
          }
          muted={running.length === 0}
          onClick={() => setScreen('activity')}
        />
        {nextEvent && <Card label={t('orbital.nextEvent')} value={nextEvent.title} />}
      </div>

      <main className="orbital__stage">
        <Orb state={orbState} level={audioLevel} onClick={onToggleVoice} />

        <h1 className="orbital__wordmark">{name || t('app.name')}</h1>

        <p className="orbital__tagline" data-phrase={phrase ? 'true' : undefined}>
          {phrase ?? t('orbital.tagline')}
        </p>

        <div className="orbital__actions">
          <button type="button" className="orbital__action" onClick={() => setScreen('chat')}>
            <ChatIcon />
            {t('orbital.openChat')}
          </button>
          <button
            type="button"
            className="orbital__action"
            data-active={orbState === 'listening' ? 'true' : undefined}
            onClick={onToggleVoice}
          >
            <MicIcon />
            {t('orbital.talk')}
          </button>
        </div>
      </main>

      <div className="orbital__command">
        <CommandBar
          onSubmit={onSubmit}
          onToggleVoice={onToggleVoice}
          listening={orbState === 'listening'}
        />
      </div>
    </div>
  )
}

/** Короткая сводка: подпись и значение. */
function Card({
  label,
  value,
  muted,
  onClick,
  children,
}: {
  label: string
  value?: string
  muted?: boolean
  onClick?: () => void
  children?: ReactNode
}) {
  if (onClick) {
    return (
      <button type="button" className="card" data-muted={muted ? 'true' : undefined} onClick={onClick}>
        <span className="card__label">{label}</span>
        {value && <span className="card__value">{value}</span>}
        {children}
      </button>
    )
  }

  return (
    <div className="card" data-muted={muted ? 'true' : undefined}>
      <span className="card__label">{label}</span>
      {value && <span className="card__value">{value}</span>}
      {children}
    </div>
  )
}

/**
 * Погода (`docs/GAPS.md` §6).
 *
 * По IP местонахождение не определяется: это отправка данных о том, где
 * человек находится, туда, куда он её не просил отправлять. Город берётся из
 * настройки, а если её нет — из часового пояса самого компьютера, см.
 * `city.ts`. Название показывается то, которое нашёл геокодер, чтобы догадка
 * была видна, а не выдавала себя за точное знание.
 */
function WeatherCard() {
  const [weather, setWeather] = useState<Weather | null>(null)
  const [city, setCity] = useState<string | null>(null)

  useEffect(() => {
    if (!isTauri()) return

    let cancelled = false

    void settingGet('everyday.city')
      .then((saved) => {
        const name = saved?.trim() || cityFromTimeZone(localTimeZone())
        if (cancelled || !name) return
        setCity(name)
        return weatherGet(name).then((value) => {
          if (!cancelled) setWeather(value)
        })
      })
      // Нет сети, сервис молчит или города с таким именем не существует —
      // виджета просто нет. Показывать ошибку там, где человек ждёт покоя,
      // хуже, чем не показывать ничего: погода — не то, из-за чего стоит
      // тревожить.
      .catch(() => {
        if (!cancelled) setCity(null)
      })

    return () => {
      cancelled = true
    }
  }, [])

  // Пока погода не пришла, виджета нет: пустая карточка с названием города
  // сообщает ровно ничего.
  if (!city || !weather) return null

  return (
    <div className="card">
      <span className="card__label">{weather.place}</span>
      <span className="card__big">{Math.round(weather.temperature)}°</span>
      <span className="card__value">{weather.description}</span>
    </div>
  )
}

/**
 * Память машины.
 *
 * Показывается занятая доля, а не только абсолютные байты: «6.1 из 16 ГБ»
 * человек всё равно переводит в долю, прежде чем сделать вывод.
 */
function MemoryCard() {
  const t = useT()
  const [info, setInfo] = useState<SystemInfo | null>(null)

  useEffect(() => {
    if (!isTauri()) return

    let cancelled = false

    const read = () => {
      void systemInfo()
        .then((value) => {
          if (!cancelled) setInfo(value)
        })
        .catch(() => undefined)
    }

    read()
    // Раз в пятнадцать секунд: память меняется медленно, а опрос чаще — это
    // разбуженный рендер ради цифры, которая не изменилась.
    const timer = setInterval(read, 15_000)

    return () => {
      cancelled = true
      clearInterval(timer)
    }
  }, [])

  if (!info || info.totalMemoryBytes === 0) return null

  const used = info.totalMemoryBytes - info.availableMemoryBytes
  const share = Math.min(1, Math.max(0, used / info.totalMemoryBytes))
  const gb = (bytes: number) => (bytes / 1024 ** 3).toFixed(1)

  return (
    <div className="card">
      <span className="card__label">{t('orbital.memory')}</span>
      <span className="card__value">
        {Math.round(share * 100)}% · {gb(used)} / {gb(info.totalMemoryBytes)} {t('unit.gb')}
      </span>
      <span className="card__bar">
        <span className="card__fill" style={{ inlineSize: `${share * 100}%` }} />
      </span>
    </div>
  )
}

const stroke = {
  stroke: 'currentColor',
  strokeWidth: 1.6,
  strokeLinecap: 'round' as const,
  strokeLinejoin: 'round' as const,
  fill: 'none',
}

function ChatIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M4 6.5A2.5 2.5 0 0 1 6.5 4h11A2.5 2.5 0 0 1 20 6.5v7a2.5 2.5 0 0 1-2.5 2.5H9l-5 4z"
        {...stroke}
      />
    </svg>
  )
}

function MicIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true">
      <rect x="9" y="3" width="6" height="11" rx="3" {...stroke} />
      <path d="M5 11a7 7 0 0 0 14 0M12 18v3" {...stroke} />
    </svg>
  )
}
