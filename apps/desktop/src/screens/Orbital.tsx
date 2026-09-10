import { useEffect, useMemo, useState } from 'react'

import { isTauri, settingGet, weatherGet, type Weather } from '../bridge'
import { CommandBar } from '../design-system/components/CommandBar'
import { Orb } from '../design-system/components/Orb'
import { ActivityChip, StatusPill } from '../design-system/components/Pills'
import { useI18n, useT } from '../i18n'
import { activeTasks, useUiStore } from '../state/store'
import type { OrbState } from '../state/types'
import './Orbital.css'

/** Реплика под Orb для состояний, у которых она предопределена (ТЗ §13). */
const HEADLINE_KEY: Partial<Record<OrbState, string>> = {
  idle: 'orbital.greeting',
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
  const connection = useUiStore((s) => s.connection)
  const nextEvent = useUiStore((s) => s.nextEvent)
  const tasks = useUiStore((s) => s.tasks)

  const running = useMemo(() => activeTasks(tasks), [tasks])

  const now = useClock()

  const timeLabel = useMemo(
    () => new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' }).format(now),
    [locale, now],
  )

  const dateLabel = useMemo(
    () => new Intl.DateTimeFormat(locale, { weekday: 'short', day: 'numeric', month: 'long' }).format(now),
    [locale, now],
  )

  // Явно заданная реплика важнее шаблонной: во время задачи Yuki говорит,
  // что именно она делает («Ищу файл…»), а не общее «Работаю…».
  const key = HEADLINE_KEY[orbState]
  const phrase = headline ?? (key ? t(key) : null)

  const listening = orbState === 'listening'

  return (
    <div className="orbital">
      <header className="orbital__top">
        <span className="orbital__wordmark">{t('app.name')}</span>
        <span className="orbital__clock">{timeLabel}</span>
        <StatusPill tone={connection.online ? 'online' : 'offline'}>
          {connection.online ? connection.providerLabel : t('status.offline')}
        </StatusPill>
      </header>

      <main className="orbital__stage">
        <Orb state={orbState} level={audioLevel} onClick={onToggleVoice} />
        {phrase && <p className="orbital__headline">{phrase}</p>}
      </main>

      <div className="orbital__command">
        <CommandBar onSubmit={onSubmit} onToggleVoice={onToggleVoice} listening={listening} />
      </div>

      <footer className="orbital__strip">
        <ActivityChip label={t('orbital.today')} value={dateLabel} />
        <ActivityChip
          label={t('orbital.tasks')}
          value={
            running.length > 0
              ? t('orbital.tasksCount', { count: running.length })
              : t('orbital.noTasks')
          }
          empty={running.length === 0}
        />
        <ActivityChip
          label={t('orbital.nextEvent')}
          value={nextEvent ? nextEvent.title : t('orbital.noEvents')}
          empty={!nextEvent}
        />
        <WeatherChip />
      </footer>
    </div>
  )
}


/**
 * Погода в нижней полосе (`docs/GAPS.md` §6).
 *
 * Город берётся из настройки и не угадывается по IP: геолокация по адресу —
 * это отправка данных о местонахождении туда, куда человек её не просил
 * отправлять. Без города чип просто не показывается.
 */
function WeatherChip() {
  const t = useT()
  const [weather, setWeather] = useState<Weather | null>(null)
  const [city, setCity] = useState<string | null>(null)

  useEffect(() => {
    if (!isTauri()) return

    let cancelled = false

    void settingGet('everyday.city')
      .then((saved) => {
        const name = saved?.trim()
        if (cancelled || !name) return
        setCity(name)
        return weatherGet(name).then((value) => {
          if (!cancelled) setWeather(value)
        })
      })
      // Нет сети или сервис молчит — полоса остаётся без погоды, а не
      // показывает ошибку там, где человек ждёт покоя.
      .catch(() => undefined)

    return () => {
      cancelled = true
    }
  }, [])

  if (!city) return null

  return (
    <ActivityChip
      label={t('orbital.weather')}
      value={
        weather
          ? `${Math.round(weather.temperature)}°, ${weather.description}`
          : city
      }
      empty={!weather}
    />
  )
}

/**
 * Часы, обновляющиеся на границе минуты.
 *
 * Тикать раз в секунду ради отображения часов и минут — значит будить рендер
 * шестьдесят раз впустую, а бюджет простоя задан в ТЗ §37.
 */
function useClock(): Date {
  const [now, setNow] = useState(() => new Date())

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>

    const schedule = () => {
      const msToNextMinute = 60_000 - (Date.now() % 60_000)
      timer = setTimeout(() => {
        setNow(new Date())
        schedule()
      }, msToNextMinute)
    }

    schedule()
    return () => clearTimeout(timer)
  }, [])

  return now
}
