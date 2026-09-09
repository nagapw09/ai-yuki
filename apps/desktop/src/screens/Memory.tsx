import { useCallback, useEffect, useState } from 'react'

import {
  memoryClear,
  memoryDelete,
  memoryList,
  memorySave,
  type MemoryKind,
  type MemoryRecord,
} from '../bridge'
import { useI18n } from '../i18n'
import './Memory.css'

/** Типы памяти из ТЗ §9 в порядке от постоянного к мимолётному. */
const KINDS: { id: MemoryKind; label: string; hint: string }[] = [
  { id: 'long_term', label: 'Долгосрочная', hint: 'То, что верно всегда: имя, язык, предпочтения' },
  { id: 'episodic', label: 'Эпизоды', hint: 'Что происходило и когда' },
  { id: 'session', label: 'Сессия', hint: 'Контекст текущего разговора' },
  { id: 'short_term', label: 'Краткосрочная', hint: 'Рабочие заметки, живут около часа' },
]

/**
 * Управление памятью (ТЗ §9): просмотр, правка, удаление, полная очистка.
 *
 * Экран существует ровно потому, что ТЗ требует: пользователь должен видеть всё,
 * что Yuki о нём помнит, и мочь это изменить. Поэтому здесь нет скрытых записей
 * и нет типа памяти, который нельзя было бы открыть.
 */
export function Memory() {
  const { locale } = useI18n()
  const [records, setRecords] = useState<MemoryRecord[]>([])
  const [error, setError] = useState<string | null>(null)
  const [confirmingClear, setConfirmingClear] = useState(false)

  const reload = useCallback(async () => {
    try {
      setRecords(await memoryList())
      setError(null)
    } catch (e) {
      setError(describe(e))
    }
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  const date = new Intl.DateTimeFormat(locale, { dateStyle: 'short', timeStyle: 'short' })

  return (
    <div className="memory">
      <div className="memory__inner">
        <header className="memory__header">
          <div>
            <h2 className="memory__title">Память</h2>
            <p className="memory__hint">
              Yuki запоминает только то, что сохранила явно. Любую запись можно
              изменить или удалить — она хранится локально и никуда не отправляется.
            </p>
          </div>

          {records.length > 0 &&
            (confirmingClear ? (
              <div className="memory__confirm">
                <span>Удалить всё?</span>
                <button
                  type="button"
                  className="memory__button memory__button--danger"
                  onClick={() =>
                    void memoryClear()
                      .then(reload)
                      .finally(() => setConfirmingClear(false))
                  }
                >
                  Да, удалить
                </button>
                <button
                  type="button"
                  className="memory__button"
                  onClick={() => setConfirmingClear(false)}
                >
                  Отмена
                </button>
              </div>
            ) : (
              // Полная очистка необратима, поэтому подтверждение встроено прямо
              // в кнопку: диалог ради одного вопроса здесь избыточен.
              <button
                type="button"
                className="memory__button"
                onClick={() => setConfirmingClear(true)}
              >
                Очистить всё
              </button>
            ))}
        </header>

        {error && <p className="memory__error">{error}</p>}

        {records.length === 0 && !error && (
          <p className="memory__empty">Пока пусто. Yuki запомнит то, что вы попросите.</p>
        )}

        {KINDS.map((kind) => {
          const group = records.filter((r) => r.kind === kind.id)
          if (group.length === 0) return null

          return (
            <section className="memory__group" key={kind.id}>
              <h3 className="memory__group-title">
                {kind.label}
                <span className="memory__group-hint">{kind.hint}</span>
              </h3>

              <div className="memory__list">
                {group.map((record) => (
                  <MemoryRow
                    key={record.id}
                    record={record}
                    formatted={date.format(record.updatedAt * 1000)}
                    onChanged={reload}
                  />
                ))}
              </div>
            </section>
          )
        })}
      </div>
    </div>
  )
}

function MemoryRow({
  record,
  formatted,
  onChanged,
}: {
  record: MemoryRecord
  formatted: string
  onChanged: () => Promise<void>
}) {
  const [content, setContent] = useState(record.content)
  const [saving, setSaving] = useState(false)

  const save = async () => {
    if (content.trim() === record.content || content.trim() === '') {
      setContent(record.content)
      return
    }
    setSaving(true)
    try {
      await memorySave({
        kind: record.kind,
        content,
        ...(record.key ? { key: record.key } : {}),
        source: 'user',
      })
      await onChanged()
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="memory__row" data-saving={saving}>
      {record.key && <span className="memory__key">{record.key}</span>}

      <textarea
        className="memory__content"
        value={content}
        rows={1}
        onChange={(e) => setContent(e.target.value)}
        onBlur={() => void save()}
        spellCheck={false}
      />

      <span className="memory__meta">{formatted}</span>

      <button
        type="button"
        className="memory__delete"
        aria-label="Удалить запись"
        onClick={() => void memoryDelete(record.id).then(onChanged)}
      >
        ×
      </button>
    </div>
  )
}

function describe(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return 'Не удалось прочитать память'
}
