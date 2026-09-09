import { useEffect, useState } from 'react'

import { activityLog, type ActivityEntry } from '../bridge'
import { useI18n } from '../i18n'
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
 * Показывает время, инструмент, цель, статус, результат и длительность. Секретов
 * здесь нет по построению: команда записи журнала принимает только те поля, что
 * видны на экране, а ключи в неё попасть не могут — их нет во фронтенде вообще.
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

  const time = new Intl.DateTimeFormat(locale, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })

  return (
    <div className="activity">
      <div className="activity__inner">
        <h2 className="activity__title">Активность</h2>

        {error && <p className="activity__error">{error}</p>}

        {!error && entries.length === 0 && (
          <p className="activity__empty">Пока ничего не происходило.</p>
        )}

        <ol className="activity__list">
          {entries.map((entry) => (
            <li className="activity__row" data-status={entry.status} key={entry.id}>
              <span className="activity__time">{time.format(entry.ts * 1000)}</span>
              <span className="activity__tool">{entry.tool}</span>
              <span className="activity__status">{STATUS_LABEL[entry.status]}</span>
              <span className="activity__result" data-selectable>
                {entry.result ?? ''}
              </span>
              <span className="activity__duration">
                {entry.durationMs === null ? '' : formatDuration(entry.durationMs)}
              </span>
            </li>
          ))}
        </ol>
      </div>
    </div>
  )
}

function formatDuration(ms: number): string {
  return ms < 1000 ? `${ms} мс` : `${(ms / 1000).toFixed(1)} с`
}
