/**
 * Поля шага и редактор условия (ТЗ §16).
 *
 * Отдельным модулем, потому что их показывают два места: линейный список шагов
 * и панель выбранного узла на полотне. Держать их в одном из этих файлов
 * значило бы получить круговой импорт, а держать по копии в каждом — увидеть,
 * как редакторы расходятся на первом же добавленном виде шага.
 */

import type { Condition, Step } from '@yuki/core'

export interface ToolOption {
  id: string
  name: string
}

/**
 * Поля одного шага.
 *
 * Вынесены из списка, потому что их показывают два места: линейный список и
 * панель выбранного узла на полотне. Две копии этих полей разошлись бы на
 * первом же добавленном виде шага, и один из редакторов начал бы терять данные.
 */
export function StepFields({
  step,
  tools,
  onChange,
}: {
  step: Step
  tools: ToolOption[]
  onChange: (step: Step) => void
}) {
  if (step.kind === 'action') {
    return (
      <>
        <select
          className="commands__input"
          value={step.toolId}
          onChange={(e) => onChange({ ...step, toolId: e.target.value })}
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
              onChange({ ...step, input: JSON.parse(e.target.value) })
            } catch {
              // Пока JSON не дописан, он невалиден — это нормально, и ронять
              // ввод на каждом символе нельзя.
            }
          }}
          placeholder='Аргументы, например {"app": "Chrome"}'
          spellCheck={false}
        />
      </>
    )
  }

  if (step.kind === 'delay') {
    return (
      <input
        className="commands__input"
        type="number"
        value={step.ms}
        onChange={(e) => onChange({ ...step, ms: Number(e.target.value) || 0 })}
        placeholder="Пауза, мс"
      />
    )
  }

  return (
    <ConditionEditor
      condition={step.condition}
      onChange={(condition) => onChange({ ...step, condition })}
    />
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
