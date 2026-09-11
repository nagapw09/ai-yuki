import { useEffect, useMemo, useState } from 'react'

import { activityLog, type ActivityEntry } from '../bridge'
import { Empty } from '../design-system/components/Empty'
import { useI18n } from '../i18n'
import { formatDuration, resultSummary, toolLabel } from './activity-label'
import './Activity.css'

/** Подписи статусов из ТЗ §23. */
const STATUS_LABEL: Record<ActivityEntry['status'], string> = {
  ok: 'выполнено',
  error: 'ошибка',
  cancelled: 'отменено',
  denied: 'не разрешено',
}

/**
 * Журнал активности (ТЗ §23).
 *
 * # Почему это лента, а не таблица
 *
 * Раньше здесь была таблица из пяти колонок, и в одной из них лежал сырой
 * JSON результата. Формально в ней было всё, а прочитать было нельзя: имена
 * инструментов внутренние, результат обрезан по ширине колонки, ничего не
 * сгруппировано. Раздел выглядел выводом консоли внутри приложения.
 *
 * Теперь каждая запись — строка с человеческим названием действия и его целью;
 * полный результат раскрывается по требованию. Секретов здесь нет по
 * построению: команда записи журнала принимает только те поля, что видны на
 * экране, а ключей во фронтенде нет вообще.
 */
export function Activity() {
  const { locale } = useI18n()
  const [entries, setEntries] = useState<ActivityEntry[]>([])
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    activityLog(200)
      .then(setEntries)
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
  }, [])

  const time = useMemo(
    () => new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' }),
    [locale],
  )

  const day = useMemo(
    () => new Intl.DateTimeFormat(locale, { weekday: 'long', day: 'numeric', month: 'long' }),
    [locale],
  )

  // Группировка по дням: без неё двести записей читаются как один поток, и
  // «когда это было» приходится считать по времени в начале строки.
  const groups = useMemo(() => {
    const byDay = new Map<string, ActivityEntry[]>()

    for (const entry of entries) {
      const key = new Date(entry.ts * 1000).toDateString()
      const list = byDay.get(key)
      if (list) list.push(entry)
      else byDay.set(key, [entry])
    }

    return [...byDay.entries()]
  }, [entries])

  return (
    <div className="activity">
      <div className="activity__inner">
        <header className="page-head">
          <div>
            <h2 className="page-head__title">Активность</h2>
            <p className="page-head__hint">
              Всё, что Yuki сделала на этом компьютере: что за действие, над чем,
              чем закончилось и сколько заняло.
            </p>
          </div>
        </header>

        {error && <p className="activity__error">{error}</p>}

        {!error && entries.length === 0 && (
          <Empty
            title="Пока ничего не происходило"
            body="Здесь появится каждое действие Yuki — открытые приложения, прочитанные файлы, нажатые клавиши. Журнал ведётся всегда и никуда не отправляется."
          />
        )}

        {groups.map(([key, list]) => (
          <section className="activity__day" key={key}>
            <h3 className="activity__date">{day.format(new Date(key))}</h3>

            <ol className="activity__list">
              {list.map((entry) => (
                <Row key={entry.id} entry={entry} time={time} />
              ))}
            </ol>
          </section>
        ))}
      </div>
    </div>
  )
}

function Row({ entry, time }: { entry: ActivityEntry; time: Intl.DateTimeFormat }) {
  const summary = resultSummary(entry.result)

  // Раскрытие даётся только там, где под ним есть что-то, чего не видно в
  // строке: пустое «подробнее» — это обещание, за которым ничего нет.
  const expandable = Boolean(entry.result && summary && entry.result.trim().length > summary.length)

  return (
    <li className="activity__row" data-status={entry.status}>
      <span className="activity__dot" aria-hidden="true" />

      <div className="activity__body">
        <span className="activity__name">{toolLabel(entry.tool)}</span>
        {entry.target && (
          <span className="activity__target" data-selectable>
            {entry.target}
          </span>
        )}
        {summary && !expandable && (
          <span className="activity__summary" data-selectable>
            {summary}
          </span>
        )}
        {expandable && (
          <details className="activity__details">
            <summary className="activity__summary">{summary}</summary>
            <pre className="activity__raw" data-selectable>
              {entry.result}
            </pre>
          </details>
        )}
      </div>

      <span className="activity__status">{STATUS_LABEL[entry.status]}</span>
      <span className="activity__meta">
        {entry.durationMs !== null && (
          <span className="activity__duration">{formatDuration(entry.durationMs)}</span>
        )}
        <span className="activity__time">{time.format(entry.ts * 1000)}</span>
      </span>
    </li>
  )
}
