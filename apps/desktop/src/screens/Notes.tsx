import { useCallback, useEffect, useState } from 'react'

import { noteDelete, noteList, noteSave, type Note } from '../bridge'
import { useI18n } from '../i18n'
import './Notes.css'

/**
 * Заметки (`docs/GAPS.md` §5).
 *
 * # Почему отдельный экран, а не вкладка в памяти
 *
 * Память (ТЗ §9) — это то, что Yuki читает сама и подмешивает в контекст
 * каждого запроса. Заметка — текст, который человек написал для себя. Положить
 * их рядом значит либо утопить память в чужих черновиках, либо приучить
 * человека, что всё написанное уходит в модель.
 *
 * Экран намеренно скучный: список слева, текст справа, сохранение по уходу
 * из поля. Заметки — не место для изобретательности.
 */
export function Notes() {
  const { locale } = useI18n()
  const [notes, setNotes] = useState<Note[]>([])
  const [query, setQuery] = useState('')
  const [active, setActive] = useState<Note | null>(null)
  const [draft, setDraft] = useState('')
  const [error, setError] = useState<string | null>(null)

  const reload = useCallback(async (search: string) => {
    try {
      setNotes(await noteList(search))
      setError(null)
    } catch (e) {
      setError(describe(e))
    }
  }, [])

  useEffect(() => {
    void reload(query)
  }, [reload, query])

  const date = new Intl.DateTimeFormat(locale, { dateStyle: 'short', timeStyle: 'short' })

  /** Сохраняет черновик, если он изменился. */
  const flush = async () => {
    const body = draft.trim()

    // Пустой текст — это не «сохранить пустую заметку», а «ничего не написал».
    if (body === '' || body === active?.body) return

    try {
      const saved = await noteSave({ id: active?.id, body })
      setActive(saved)
      await reload(query)
    } catch (e) {
      setError(describe(e))
    }
  }

  const open = (note: Note | null) => {
    void flush().then(() => {
      setActive(note)
      setDraft(note?.body ?? '')
    })
  }

  return (
    <div className="notes">
      <aside className="notes__list">
        <div className="notes__toolbar">
          <input
            className="notes__search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Поиск"
            spellCheck={false}
          />
          <button type="button" className="notes__button" onClick={() => open(null)}>
            + Новая
          </button>
        </div>

        {notes.length === 0 && (
          <p className="notes__empty">
            {query ? 'Ничего не нашлось.' : 'Заметок пока нет. Можно попросить Yuki: «запиши…».'}
          </p>
        )}

        {notes.map((note) => (
          <button
            key={note.id}
            type="button"
            className="notes__item"
            data-active={note.id === active?.id}
            onClick={() => open(note)}
          >
            <span className="notes__item-title">
              {note.pinned && <span className="notes__pin" aria-hidden="true">●</span>}
              {note.title || 'Без названия'}
            </span>
            <span className="notes__item-date">{date.format(note.updatedAt * 1000)}</span>
          </button>
        ))}
      </aside>

      <section className="notes__editor">
        {error && <p className="notes__error">{error}</p>}

        <textarea
          className="notes__body"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          // Сохранение по уходу из поля, а не по кнопке: заметка, потерянная
          // из-за забытой кнопки, — это худшее, что может сделать блокнот.
          onBlur={() => void flush()}
          placeholder="Пишите здесь. Первая строка станет заголовком."
          spellCheck
        />

        {active && (
          <div className="notes__actions">
            <button
              type="button"
              className="notes__link"
              onClick={() =>
                void noteSave({ id: active.id, body: active.body, pinned: !active.pinned })
                  .then((saved) => setActive(saved))
                  .then(() => reload(query))
              }
            >
              {active.pinned ? 'открепить' : 'закрепить'}
            </button>

            <button
              type="button"
              className="notes__link notes__link--danger"
              onClick={() =>
                void noteDelete(active.id).then(() => {
                  setActive(null)
                  setDraft('')
                  return reload(query)
                })
              }
            >
              удалить
            </button>

            <span className="notes__meta">изменена {date.format(active.updatedAt * 1000)}</span>
          </div>
        )}
      </section>
    </div>
  )
}

function describe(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return 'Неизвестная ошибка'
}
