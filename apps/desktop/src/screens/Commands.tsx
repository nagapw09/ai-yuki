import { useCallback, useEffect, useMemo, useState } from 'react'

import type { Condition, Step } from '@yuki/core'

import { runById } from '../agent/commands'
import { COMMAND_TEMPLATES, type CommandTemplate } from '../agent/templates'
import { toolRegistry } from '../agent/session'
import { commandDelete, commandList, commandSave, type CommandRecord } from '../bridge'
import { Empty } from '../design-system/components/Empty'
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

  const reload = useCallback(async () => {
    try {
      setCommands(await commandList())
      setError(null)
    } catch (e) {
      setError(describe(e))
    }
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  const create = () =>
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

  return (
    <div className="commands">
      <div className="commands__inner">
        <header className="commands__header">
          <div>
            <h2 className="commands__title">Команды</h2>
            <p className="commands__hint">
              Записанная последовательность выполняется без модели: мгновенно
              и без расхода токенов. Каждый шаг проходит те же разрешения,
              что и действия Yuki.
            </p>
          </div>
          <div className="commands__actions-top">
            <button
              type="button"
              className="commands__button"
              onClick={() => setLibrary((open) => !open)}
            >
              {library ? 'Скрыть библиотеку' : 'Библиотека'}
            </button>
            <button type="button" className="commands__button" onClick={create}>
              Новая команда
            </button>
          </div>
        </header>

        {library && !editing && (
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
        )}

        {error && <p className="commands__error">{error}</p>}

        {editing ? (
          <Editor
            command={editing}
            onCancel={() => setEditing(null)}
            onSaved={async () => {
              setEditing(null)
              await reload()
            }}
          />
        ) : (
          <div className="commands__list">
            {commands.length === 0 && (
              <Empty
                title="Команд пока нет"
                body="Команда — это записанная последовательность действий. Она выполняется мгновенно и без расхода токенов, потому что модель в ней не участвует."
                action={
                  <button type="button" className="commands__button" onClick={create}>
                    Новая команда
                  </button>
                }
              />
            )}

            {commands.map((command) => (
              <Row
                key={command.id}
                command={command}
                onEdit={() => setEditing(command)}
                onChanged={reload}
              />
            ))}
          </div>
        )}
      </div>
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

function Row({
  command,
  onEdit,
  onChanged,
}: {
  command: CommandRecord
  onEdit: () => void
  onChanged: () => Promise<void>
}) {
  // Ход выполнения виден в чате, но переключать туда экран по нажатию кнопки
  // на этом же экране — значит уносить пользователя оттуда, где он работает.
  // Поэтому итог показывается здесь же.
  const [note, setNote] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const run = () => {
    setBusy(true)
    setNote(null)
    void runById(command.id)
      .then((outcome) =>
        setNote(
          outcome.ok
            ? `выполнено, шагов: ${outcome.executed}`
            : (outcome.error ?? 'не выполнено'),
        ),
      )
      .finally(() => setBusy(false))
  }

  return (
    <div className="command" data-enabled={command.enabled}>
      <div className="command__head">
        <span className="command__name">{command.name}</span>
        <span className="command__trigger">
          {TRIGGER_LABEL[command.triggerKind]}
          {command.triggerKind === 'phrase' && command.phrase ? `: «${command.phrase}»` : ''}
          {command.triggerKind === 'hotkey' && command.hotkey ? `: ${command.hotkey}` : ''}
        </span>
        <span className="command__steps">шагов: {command.steps.length}</span>
      </div>

      <div className="command__actions">
        <button type="button" className="commands__link" disabled={busy} onClick={run}>
          {busy ? 'выполняю…' : 'выполнить'}
        </button>
        <button type="button" className="commands__link" onClick={onEdit}>
          изменить
        </button>
        <button
          type="button"
          className="commands__link"
          onClick={() =>
            void commandSave({ ...command, enabled: !command.enabled }).then(onChanged)
          }
        >
          {command.enabled ? 'выключить' : 'включить'}
        </button>
        <button
          type="button"
          className="commands__link commands__link--danger"
          onClick={() => void commandDelete(command.id).then(onChanged)}
        >
          удалить
        </button>
        {note && <span className="command__note">{note}</span>}
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

  return (
    <div className="editor">
      <div className="editor__row">
        <input
          className="commands__input"
          value={draft.name}
          onChange={(e) => setDraft({ ...draft, name: e.target.value })}
          placeholder="Название команды"
        />
        <select
          className="commands__input"
          value={draft.triggerKind}
          onChange={(e) =>
            setDraft({ ...draft, triggerKind: e.target.value as CommandRecord['triggerKind'] })
          }
        >
          <option value="phrase">Запуск по фразе</option>
          <option value="hotkey">Запуск по сочетанию</option>
          <option value="startup">При запуске Yuki</option>
          <option value="manual">Только вручную</option>
        </select>
      </div>

      {draft.triggerKind === 'phrase' && (
        <input
          className="commands__input"
          value={draft.phrase ?? ''}
          onChange={(e) => setDraft({ ...draft, phrase: e.target.value })}
          placeholder="Фраза, например «рабочий режим»"
        />
      )}

      {draft.triggerKind === 'hotkey' && (
        <input
          className="commands__input"
          value={draft.hotkey ?? ''}
          onChange={(e) => setDraft({ ...draft, hotkey: e.target.value })}
          placeholder="Сочетание, например Ctrl+Alt+W"
          spellCheck={false}
        />
      )}

      <StepList
        steps={draft.steps as Step[]}
        tools={tools}
        depth={0}
        onChange={(steps) => setDraft({ ...draft, steps })}
      />

      <div className="command__actions">
        <button
          type="button"
          className="commands__button"
          disabled={draft.name.trim() === ''}
          onClick={save}
        >
          Сохранить
        </button>
        <button type="button" className="commands__link" onClick={onCancel}>
          отмена
        </button>
        {note && <span className="command__note">{note}</span>}
      </div>
    </div>
  )
}

interface ToolOption {
  id: string
  name: string
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

            {step.kind === 'action' && (
              <>
                <select
                  className="commands__input"
                  value={step.toolId}
                  onChange={(e) => replace(index, { ...step, toolId: e.target.value })}
                >
                  {/* Инструмент из команды мог исчезнуть вместе с возможностью;
                      показываем его как есть, чтобы правка не подменила шаг молча. */}
                  {!tools.some((t) => t.id === step.toolId) && (
                    <option value={step.toolId}>{step.toolId} — недоступен</option>
                  )}
                  {tools.map((tool) => (
                    <option key={tool.id} value={tool.id}>
                      {tool.name}
                    </option>
                  ))}
                </select>
                <input
                  className="commands__input"
                  value={JSON.stringify(step.input)}
                  onChange={(e) => {
                    try {
                      replace(index, { ...step, input: JSON.parse(e.target.value) })
                    } catch {
                      // Пока JSON не дописан, он невалиден — это нормально,
                      // и ронять ввод на каждом символе нельзя.
                    }
                  }}
                  placeholder='Аргументы, например {"app": "Chrome"}'
                  spellCheck={false}
                />
              </>
            )}

            {step.kind === 'delay' && (
              <input
                className="commands__input"
                type="number"
                value={step.ms}
                onChange={(e) => replace(index, { ...step, ms: Number(e.target.value) || 0 })}
                placeholder="Пауза, мс"
              />
            )}

            {step.kind === 'if' && (
              <ConditionEditor
                condition={step.condition}
                onChange={(condition) => replace(index, { ...step, condition })}
              />
            )}

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

const OPS: { value: Condition['op']; label: string }[] = [
  { value: 'contains', label: 'содержит' },
  { value: 'not_contains', label: 'не содержит' },
  { value: 'equals', label: 'равно' },
  { value: 'not_equals', label: 'не равно' },
  { value: 'empty', label: 'пусто' },
  { value: 'not_empty', label: 'не пусто' },
]

function ConditionEditor({
  condition,
  onChange,
}: {
  condition: Condition
  onChange: (condition: Condition) => void
}) {
  const needsRight = condition.op !== 'empty' && condition.op !== 'not_empty'

  return (
    <>
      <input
        className="commands__input"
        value={condition.left}
        onChange={(e) => onChange({ ...condition, left: e.target.value })}
        placeholder="{{результат}}"
        spellCheck={false}
      />
      <select
        className="commands__input commands__input--narrow"
        value={condition.op}
        onChange={(e) => onChange({ ...condition, op: e.target.value as Condition['op'] })}
      >
        {OPS.map((op) => (
          <option key={op.value} value={op.value}>
            {op.label}
          </option>
        ))}
      </select>
      {needsRight && (
        <input
          className="commands__input"
          value={condition.right ?? ''}
          onChange={(e) => onChange({ ...condition, right: e.target.value })}
          placeholder="значение"
        />
      )}
    </>
  )
}

function describe(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return 'Не удалось сохранить команду'
}
