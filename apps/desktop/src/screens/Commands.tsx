import { useCallback, useEffect, useMemo, useState } from 'react'

import type { Step, Trigger } from '@yuki/core'

import { runById } from '../agent/commands'
import { COMMAND_TEMPLATES, type CommandTemplate } from '../agent/templates'
import { toolRegistry } from '../agent/session'
import { commandDelete, commandList, commandSave, type CommandRecord } from '../bridge'
import { Empty } from '../design-system/components/Empty'
import { CommandCanvas } from './CommandCanvas'
import { StepFields, type ToolOption } from './CommandFields'
import './Commands.css'

/**
 * Команды и автоматизации (ТЗ §16).
 *
 * Редактор линейный, а не узловой: визуальный конструктор со связями — это
 * MVP-2 (ТЗ §39), и делать его вместо работающего списка шагов значит отложить
 * саму возможность записать команду. Ветвление при этом есть: шаг «если»
 * содержит вложенные списки, и движок понимает любую глубину.
 */
export function Commands() {
  const [commands, setCommands] = useState<CommandRecord[]>([])
  const [editing, setEditing] = useState<CommandRecord | null>(null)
  const [library, setLibrary] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [query, setQuery] = useState('')

  const reload = useCallback(async () => {
    try {
      const list = await commandList()
      setCommands(list)
      setEditing(current => current ?? list[0] ?? null)
      setError(null)
    } catch (e) {
      setError(describe(e))
    }
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  const create = () => {
    setLibrary(false)
    setEditing({
      id: `cmd-${Date.now().toString(36)}`,
      name: '',
      description: '',
      triggerKind: 'phrase',
      phrase: '',
      hotkey: null,
      enabled: true,
      steps: [],
    })
  }

  const toggleEnabled = async (command: CommandRecord, enabled: boolean) => {
    try {
      await commandSave({ ...command, enabled })
      setEditing(current => current?.id === command.id ? { ...current, enabled } : current)
      await reload()
    } catch (error) {
      setError(describe(error))
    }
  }

  const shown = (kind: CommandRecord['triggerKind']) =>
    commands.filter(c => c.triggerKind === kind && c.name.toLocaleLowerCase().includes(query.toLocaleLowerCase()))

  // Раскладка Astra: слева список с переключателями, справа редактор с
  // полотном на всю оставшуюся высоту. Отдельного списка карточек с кнопками
  // «выполнить / изменить / выключить» больше нет — всё это в редакторе.
  return (
    <div className="commands">
      <aside className="commands__sidebar">
        <div className="commands__side-head">
          <h2>Команды</h2>
          <button type="button" className="commands__icon" title="Готовые команды" aria-label="Готовые команды" data-active={library} onClick={() => setLibrary(open => !open)}>
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M4 5h6v14H4zM14 5h6v14h-6z" /></svg>
          </button>
          <button type="button" className="commands__icon commands__icon--primary" title="Новая команда" aria-label="Новая команда" onClick={create}>
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>
          </button>
        </div>
        <input className="commands__input" aria-label="Поиск команд" placeholder="Поиск…" value={query} onChange={e => setQuery(e.target.value)} />
        <div className="commands__groups">
          {(['phrase', 'hotkey', 'startup', 'manual'] as const).map(kind => {
            const group = shown(kind)
            return group.length > 0 && <section key={kind}>
              <h3><span>{TRIGGER_LABEL[kind]}</span><b>{group.length}</b></h3>
              {group.map(c => <div className="commands__nav-row" data-selected={!library && editing?.id === c.id} key={c.id}>
                <button onClick={() => { setLibrary(false); setEditing(c) }}>
                  <strong>{c.name || 'Без названия'}</strong>
                  <small>{c.triggerKind === 'phrase' && c.phrase ? `«${c.phrase}» · ` : c.hotkey ? `${c.hotkey} · ` : ''}{c.steps.length} действ.</small>
                </button>
                <input type="checkbox" role="switch" className="commands__switch" aria-label={`Включить ${c.name}`} checked={c.enabled} onChange={e => void toggleEnabled(c, e.target.checked)} />
              </div>)}
            </section>
          })}
          {!commands.length && <p className="commands__hint">Команд пока нет. Начните с готовой — кнопка слева от «+».</p>}
        </div>
      </aside>

      <main className="commands__detail">
        {error && <p className="commands__error">{error}</p>}
        {library ? (
          <Library
            onPick={(template) => {
              setLibrary(false)
              // Шаблон открывается в редакторе, а не сохраняется сразу: в
              // половине из них надо поменять названия приложений или адреса.
              setEditing({
                id: `cmd-${Date.now().toString(36)}`,
                name: template.name,
                description: template.description,
                triggerKind: template.triggerKind,
                phrase: template.phrase ?? '',
                hotkey: null,
                enabled: true,
                steps: template.steps as CommandRecord['steps'],
              })
            }}
          />
        ) : editing ? (
          <Editor
            key={editing.id}
            command={editing}
            onCancel={() => setEditing(null)}
            onSaved={async () => {
              await reload()
            }}
          />
        ) : (
          <Empty
            title="Команда не выбрана"
            body="Команда — записанная последовательность действий. Выполняется мгновенно и без модели: скажите фразу или нажмите сочетание."
            action={<button type="button" className="commands__button" onClick={create}>Новая команда</button>}
          />
        )}
      </main>
    </div>
  )
}

const TRIGGER_LABEL: Record<CommandRecord['triggerKind'], string> = {
  phrase: 'по фразе',
  hotkey: 'по сочетанию',
  startup: 'при запуске',
  manual: 'вручную',
}


/**
 * Библиотека готовых команд (`docs/GAPS.md` §9).
 *
 * Заменяет ту функцию маркетплейса, которую отказ от него забрал заодно:
 * показать, что вообще бывает. Ничего не скачивается: шаблоны идут
 * вместе с приложением и после добавления становятся обычными командами
 * пользователя — со своими правками и без связи с источником.
 */
function Library({ onPick }: { onPick: (template: CommandTemplate) => void }) {
  return (
    <div className="commands__library">
      <p className="commands__hint">
        Готовые заготовки. После добавления это обычная ваша команда:
        правьте шаги, меняйте фразу, удаляйте — ничего не обновится извне.
      </p>

      <div className="commands__templates">
        {COMMAND_TEMPLATES.map((template) => (
          <button
            key={template.id}
            type="button"
            className="commands__template"
            onClick={() => onPick(template)}
          >
            <span className="commands__template-name">{template.name}</span>
            <span className="commands__template-text">{template.description}</span>
            {template.adjust && (
              /* Честно предупреждаем, что из коробки оно ещё не работает: шаблон
                 с чужими названиями приложений просто упадёт. */
              <span className="commands__template-adjust">{template.adjust}</span>
            )}
          </button>
        ))}
      </div>
    </div>
  )
}

function Editor({
  command,
  onCancel,
  onSaved,
}: {
  command: CommandRecord
  onCancel: () => void
  onSaved: () => Promise<void>
}) {
  const [draft, setDraft] = useState<CommandRecord>(command)
  const [note, setNote] = useState<string | null>(null)
  const [running, setRunning] = useState(false)

  useEffect(() => {
    setDraft(current => ({ ...current, enabled: command.enabled }))
  }, [command.enabled])

  // Полотно по умолчанию: на нём видно форму команды целиком, а ветвления
  // читаются как ветвления, а не как отступ в списке. Список остаётся — на
  // длинной линейной команде он быстрее, и правится в нём всё сразу, а не по
  // одному выбранному узлу.
  const [view, setView] = useState<'canvas' | 'list'>('canvas')

  // Список инструментов берётся из живого реестра: в нём уже есть и встроенные,
  // и всё, что принесли подключённые возможности.
  const tools = useMemo(
    () =>
      toolRegistry()
        .list()
        .map((t) => ({ id: t.id, name: t.name }))
        .sort((a, b) => a.name.localeCompare(b.name)),
    [],
  )

  const save = () => {
    setNote(null)
    commandSave(draft)
      .then(onSaved)
      .catch((e: unknown) => setNote(describe(e)))
  }

  const run = () => {
    setRunning(true)
    setNote(null)
    void commandSave(draft)
      .then(() => runById(draft.id))
      .then(result => {
        setNote(result.ok ? `Выполнено действий: ${result.executed}` : result.error || 'Не выполнено')
        return onSaved()
      })
      .catch(e => setNote(describe(e)))
      .finally(() => setRunning(false))
  }

  return (
    <div className="editor">
      <div className="editor__head">
        <input
          className="commands__input editor__name"
          aria-label="Название команды"
          value={draft.name}
          onChange={(e) => setDraft({ ...draft, name: e.target.value })}
          placeholder="Название команды"
        />
        <button type="button" className="commands__button commands__button--go" disabled={running || !draft.name.trim() || !draft.steps.length} onClick={run}>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M8 5v14l11-7z" /></svg>
          {running ? 'Выполняется…' : 'Запустить'}
        </button>
        <button type="button" className="commands__button" disabled={draft.name.trim() === ''} onClick={save}>Сохранить</button>
        <button type="button" className="commands__icon commands__icon--danger" title="Удалить команду" aria-label="Удалить команду" disabled={running} onClick={() => void commandDelete(draft.id).then(() => { onCancel(); return onSaved() }).catch(e => setNote(describe(e)))}>
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true"><path d="M5 7h14M10 7V4h4v3M7 7l1 13h8l1-13" /></svg>
        </button>
      </div>

      <div className="editor__bar">
        <select
          className="commands__input"
          aria-label="Когда запускать"
          value={draft.triggerKind}
          onChange={(e) =>
            setDraft({ ...draft, triggerKind: e.target.value as CommandRecord['triggerKind'] })
          }
        >
          <option value="phrase">По фразе</option>
          <option value="hotkey">По сочетанию</option>
          <option value="startup">При запуске Yuki</option>
          <option value="manual">Только вручную</option>
        </select>
        {draft.triggerKind === 'phrase' && (
          <input
            className="commands__input"
            aria-label="Фраза"
            value={draft.phrase ?? ''}
            onChange={(e) => setDraft({ ...draft, phrase: e.target.value })}
            placeholder="Фраза, например «рабочий режим»"
          />
        )}
        {draft.triggerKind === 'hotkey' && (
          <input
            className="commands__input"
            aria-label="Сочетание клавиш"
            value={draft.hotkey ?? ''}
            onChange={(e) => setDraft({ ...draft, hotkey: e.target.value })}
            placeholder="Например Ctrl+Alt+W"
            spellCheck={false}
          />
        )}
        <span className="editor__spacer" />
        {note && <span className="command__note">{note}</span>}
        <div className="editor__views" role="group" aria-label="Вид">
          <button type="button" data-active={view === 'canvas'} onClick={() => setView('canvas')}>Полотно</button>
          <button type="button" data-active={view === 'list'} onClick={() => setView('list')}>Список</button>
        </div>
      </div>

      <div className="editor__stage">
        {view === 'canvas' ? (
          <CommandCanvas
            steps={draft.steps as Step[]}
            trigger={triggerOf(draft)}
            tools={tools}
            onChange={(steps) => setDraft({ ...draft, steps: steps as Step[] })}
          />
        ) : (
          <div className="editor__list">
            <StepList
              steps={draft.steps as Step[]}
              tools={tools}
              depth={0}
              onChange={(steps) => setDraft({ ...draft, steps })}
            />
          </div>
        )}
      </div>
    </div>
  )
}

/** Редактор списка шагов; вызывает сам себя для веток «если». */
function StepList({
  steps,
  tools,
  depth,
  onChange,
}: {
  steps: Step[]
  tools: ToolOption[]
  depth: number
  onChange: (steps: Step[]) => void
}) {
  const replace = (index: number, step: Step) =>
    onChange(steps.map((s, i) => (i === index ? step : s)))

  const remove = (index: number) => onChange(steps.filter((_, i) => i !== index))

  const add = (step: Step) => onChange([...steps, step])

  /**
   * Перестановка шага.
   *
   * Порядок в автоматизации — это смысл, а не оформление: скопировать
   * текст надо до вставки, а не после. Без перестановки единственный
   * способ вставить шаг в середину — стереть хвост и набрать заново.
   */
  const move = (index: number, delta: number) => {
    const target = index + delta
    if (target < 0 || target >= steps.length) return

    const step = steps[index]
    if (!step) return

    const next = [...steps]
    next.splice(index, 1)
    next.splice(target, 0, step)
    onChange(next)
  }

  /** Копия шага рядом: чаще всего следующий шаг — почти такой же. */
  const duplicate = (index: number) => {
    const step = steps[index]
    if (!step) return

    const next = [...steps]
    // Глубокая копия: у ветвления внутри свои шаги, и общий массив
    // превратил бы правку копии в правку оригинала.
    next.splice(index + 1, 0, structuredClone(step))
    onChange(next)
  }

  return (
    <div className="steps" data-depth={depth}>
      {steps.map((step, index) => (
        <div className="step" key={index}>
          <div className="step__head">
            <span className="step__index">{index + 1}</span>

            <StepFields step={step} tools={tools} onChange={(next) => replace(index, next)} />

            <button
              type="button"
              className="commands__link"
              disabled={index === 0}
              title="выше"
              onClick={() => move(index, -1)}
            >
              ↑
            </button>
            <button
              type="button"
              className="commands__link"
              disabled={index === steps.length - 1}
              title="ниже"
              onClick={() => move(index, 1)}
            >
              ↓
            </button>
            <button
              type="button"
              className="commands__link"
              title="дублировать"
              onClick={() => duplicate(index)}
            >
              ⧉
            </button>
            <button
              type="button"
              className="commands__link commands__link--danger"
              onClick={() => remove(index)}
            >
              убрать
            </button>
          </div>

          {step.kind === 'if' && depth < 2 && (
            <div className="step__branches">
              <div className="step__branch">
                <span className="step__branch-title">если да</span>
                <StepList
                  steps={step.then as Step[]}
                  tools={tools}
                  depth={depth + 1}
                  onChange={(then) => replace(index, { ...step, then })}
                />
              </div>
              <div className="step__branch">
                <span className="step__branch-title">иначе</span>
                <StepList
                  steps={(step.otherwise ?? []) as Step[]}
                  tools={tools}
                  depth={depth + 1}
                  onChange={(otherwise) => replace(index, { ...step, otherwise })}
                />
              </div>
            </div>
          )}
        </div>
      ))}

      <div className="steps__add">
        <button
          type="button"
          className="commands__link"
          onClick={() =>
            add({ kind: 'action', toolId: tools[0]?.id ?? 'open_app', input: {} })
          }
        >
          + действие
        </button>
        <button
          type="button"
          className="commands__link"
          onClick={() => add({ kind: 'delay', ms: 500 })}
        >
          + пауза
        </button>
        {depth < 2 && (
          <button
            type="button"
            className="commands__link"
            onClick={() =>
              add({
                kind: 'if',
                condition: { left: '', op: 'contains', right: '' },
                then: [],
              })
            }
          >
            + если
          </button>
        )}
      </div>
    </div>
  )
}

/**
 * Триггер записи в том виде, в каком его понимает ядро.
 *
 * В базе он разложен по полям — вид, фраза, сочетание, — потому что колонки
 * проще искать и мигрировать. Ядру нужно размеченное объединение, и собирать
 * его приходится здесь.
 */
function triggerOf(command: CommandRecord): Trigger {
  switch (command.triggerKind) {
    case 'phrase':
      return { kind: 'phrase', phrase: command.phrase ?? '' }
    case 'hotkey':
      return { kind: 'hotkey', shortcut: command.hotkey ?? '' }
    case 'startup':
      return { kind: 'startup' }
    default:
      return { kind: 'manual' }
  }
}

function describe(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return 'Не удалось сохранить команду'
}
