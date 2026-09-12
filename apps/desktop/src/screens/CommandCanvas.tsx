/**
 * Полотно конструктора команд (ТЗ §16, ТЗ §39).
 *
 * # Что здесь можно, а что нельзя — и почему
 *
 * Узлы нельзя расставлять мышью, и связи нельзя тянуть куда попало. Команда —
 * это дерево шагов: движок идёт по списку сверху вниз и заходит в одну из двух
 * ветвей `если`. Свободный граф позволил бы нарисовать то, что выполнить
 * невозможно, и объяснять потом, почему красивая схема не запускается.
 *
 * Поэтому положение узлов считается из дерева (`layoutCommand`), а
 * перетаскивание переносит шаг в другое место дерева — полотно перекладывается
 * само. Всё, что нарисовано, выполнимо; всё, что выполнимо, можно нарисовать.
 *
 * # Почему без библиотеки узловых редакторов
 *
 * Готовые решения рассчитаны как раз на свободный граф: их сила — произвольные
 * связи и позиции, то есть именно то, что здесь запрещено. Взамен пришли бы
 * сотни килобайт в сборку и своя модель данных рядом с нашей.
 */

import {
  countSteps,
  layoutCommand,
  slotNear,
  triggerLabel,
  type CanvasNode,
  type CanvasSlot,
  type Step,
  type StepPath,
  type Trigger,
  insertStep,
  moveStep,
  removeStep,
  replaceStep,
  stepAt,
} from '@yuki/core'
import { useCallback, useMemo, useRef, useState } from 'react'

import { StepFields, type ToolOption } from './CommandFields'
import './CommandCanvas.css'

/** Границы масштаба: дальше полотно либо нечитаемо, либо бессмысленно крупно. */
const MIN_SCALE = 0.4
const MAX_SCALE = 1.6

export interface CommandCanvasProps {
  steps: readonly Step[]
  trigger: Trigger
  tools: ToolOption[]
  onChange: (steps: readonly Step[]) => void
}

export function CommandCanvas({ steps, trigger, tools, onChange }: CommandCanvasProps) {
  const canvas = useMemo(() => layoutCommand(steps), [steps])

  const viewport = useRef<HTMLDivElement | null>(null)
  const [view, setView] = useState({ x: 0, y: 0, scale: 1 })
  const [selected, setSelected] = useState<StepPath | null>(null)
  const [palette, setPalette] = useState(false)

  /** Что тащим и куда попадём, если отпустить. */
  const [drag, setDrag] = useState<{ path: StepPath; slot: CanvasSlot | null } | null>(null)

  /** Перетаскивание пустого места — это панорама. */
  const pan = useRef<{ x: number; y: number } | null>(null)

  const toCanvas = useCallback(
    (clientX: number, clientY: number) => {
      const box = viewport.current?.getBoundingClientRect()
      if (!box) return { x: 0, y: 0 }

      return {
        x: (clientX - box.left - view.x) / view.scale,
        y: (clientY - box.top - view.y) / view.scale,
      }
    },
    [view],
  )

  const selectedStep = selected ? stepAt(steps, selected) : undefined

  /** Куда класть новый шаг: рядом с выбранным, иначе в конец. */
  const insertionPoint = (): { list: StepPath; index: number } => {
    if (!selected) return { list: [], index: steps.length }

    const last = selected[selected.length - 1]
    if (typeof last !== 'number') return { list: [], index: steps.length }

    return { list: selected.slice(0, -1), index: last + 1 }
  }

  const add = (step: Step) => {
    const { list, index } = insertionPoint()
    onChange(insertStep(steps, list, index, step))
    setPalette(false)
  }

  const zoom = (delta: number) =>
    setView((current) => ({
      ...current,
      scale: Math.min(MAX_SCALE, Math.max(MIN_SCALE, current.scale + delta)),
    }))

  /** Вписывает полотно в окно — им же чинят «уехало куда-то». */
  const fit = () => {
    const box = viewport.current?.getBoundingClientRect()
    if (!box) return

    const scale = Math.min(
      MAX_SCALE,
      Math.max(MIN_SCALE, Math.min(box.width / canvas.width, box.height / canvas.height)),
    )

    setView({
      x: (box.width - canvas.width * scale) / 2,
      y: 0,
      scale,
    })
  }

  return (
    <div className="canvas">
      <div className="canvas__bar">
        <button type="button" className="commands__button" onClick={() => setPalette((v) => !v)}>
          + Узлы
        </button>
        <button type="button" className="commands__link" onClick={fit}>
          вписать
        </button>
        <button type="button" className="commands__link" onClick={() => zoom(0.15)}>
          крупнее
        </button>
        <button type="button" className="commands__link" onClick={() => zoom(-0.15)}>
          мельче
        </button>
        <span className="canvas__count">шагов: {countSteps(steps)}</span>
      </div>

      {palette && <Palette tools={tools} onPick={add} />}

      <div
        className="canvas__viewport"
        ref={viewport}
        data-dragging={drag ? 'true' : undefined}
        onPointerDown={(event) => {
          // Панорама начинается только с пустого места: иначе тяга за узел
          // одновременно двигала бы и узел, и полотно.
          if ((event.target as HTMLElement).closest('.canvas__node')) return
          pan.current = { x: event.clientX - view.x, y: event.clientY - view.y }
          event.currentTarget.setPointerCapture(event.pointerId)
        }}
        onPointerMove={(event) => {
          if (pan.current) {
            setView((current) => ({
              ...current,
              x: event.clientX - pan.current!.x,
              y: event.clientY - pan.current!.y,
            }))
            return
          }

          if (drag) {
            const point = toCanvas(event.clientX, event.clientY)
            setDrag({ ...drag, slot: slotNear(canvas, point) })
          }
        }}
        onPointerUp={() => {
          pan.current = null

          if (drag) {
            if (drag.slot) {
              onChange(moveStep(steps, drag.path, drag.slot.list, drag.slot.index))
            }
            setDrag(null)
          }
        }}
        onPointerLeave={() => {
          pan.current = null
          setDrag(null)
        }}
        onWheel={(event) => {
          event.preventDefault()
          zoom(event.deltaY > 0 ? -0.1 : 0.1)
        }}
      >
        <div
          className="canvas__layer"
          style={{
            width: canvas.width,
            height: canvas.height,
            transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})`,
          }}
        >
          <svg className="canvas__edges" width={canvas.width} height={canvas.height}>
            {canvas.edges.map((edge, index) => (
              <g key={index}>
                <path className="canvas__edge" d={curve(edge.from, edge.to)} />
                {edge.label && (
                  <text
                    className="canvas__edge-label"
                    x={(edge.from.x + edge.to.x) / 2}
                    y={(edge.from.y + edge.to.y) / 2 - 4}
                    textAnchor="middle"
                  >
                    {edge.label}
                  </text>
                )}
              </g>
            ))}
          </svg>

          {canvas.slots.map((slot, index) => (
            <span
              key={index}
              className="canvas__slot"
              data-active={
                drag?.slot?.list === slot.list && drag?.slot?.index === slot.index
                  ? 'true'
                  : undefined
              }
              style={{ left: slot.x, top: slot.y }}
            />
          ))}

          {canvas.nodes.map((node) => (
            <Node
              key={node.path.join('.') || 'trigger'}
              node={node}
              label={
                node.kind === 'trigger'
                  ? triggerLabel(trigger)
                  : stepLabel(stepAt(steps, node.path), tools)
              }
              selected={same(node.path, selected)}
              dragging={same(node.path, drag?.path ?? null)}
              onSelect={() => setSelected(node.kind === 'trigger' ? null : node.path)}
              onDragStart={() => {
                if (node.kind === 'trigger') return
                setSelected(node.path)
                setDrag({ path: node.path, slot: null })
              }}
            />
          ))}
        </div>
      </div>

      {selected && selectedStep && (
        <div className="canvas__panel">
          <div className="canvas__panel-head">
            <span className="canvas__panel-title">{stepLabel(selectedStep, tools)}</span>
            <button type="button" className="commands__link" onClick={() => setSelected(null)}>
              закрыть
            </button>
          </div>

          <div className="canvas__panel-fields">
            <StepFields
              step={selectedStep}
              tools={tools}
              onChange={(next) => onChange(replaceStep(steps, selected, next))}
            />
          </div>

          <div className="canvas__panel-actions">
            <button
              type="button"
              className="commands__link"
              onClick={() => {
                const parts = selected[selected.length - 1]
                if (typeof parts !== 'number') return
                // Глубокая копия: у ветвления внутри свои шаги, и общий массив
                // превратил бы правку копии в правку оригинала.
                onChange(
                  insertStep(
                    steps,
                    selected.slice(0, -1),
                    parts + 1,
                    structuredClone(selectedStep) as Step,
                  ),
                )
              }}
            >
              дублировать
            </button>
            <button
              type="button"
              className="commands__link commands__link--danger"
              onClick={() => {
                onChange(removeStep(steps, selected))
                setSelected(null)
              }}
            >
              удалить
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

/** Палитра узлов: управление отдельно от действий, как в любом конструкторе. */
function Palette({
  tools,
  onPick,
}: {
  tools: ToolOption[]
  onPick: (step: Step) => void
}) {
  const [query, setQuery] = useState('')

  const found = useMemo(() => {
    const needle = query.trim().toLowerCase()
    if (!needle) return tools
    return tools.filter(
      (tool) =>
        tool.name.toLowerCase().includes(needle) || tool.id.toLowerCase().includes(needle),
    )
  }, [tools, query])

  return (
    <div className="palette">
      <p className="palette__group">Управление</p>

      <button
        type="button"
        className="palette__item"
        onClick={() =>
          onPick({
            kind: 'if',
            condition: { left: '', op: 'not_empty' },
            then: [],
          })
        }
      >
        Условие
      </button>

      <button
        type="button"
        className="palette__item"
        onClick={() => onPick({ kind: 'delay', ms: 500 })}
      >
        Задержка
      </button>

      <p className="palette__group">Действия</p>

      <input
        className="commands__input commands__input--narrow"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Поиск"
        spellCheck={false}
      />

      <div className="palette__list">
        {found.map((tool) => (
          <button
            key={tool.id}
            type="button"
            className="palette__item"
            onClick={() => onPick({ kind: 'action', toolId: tool.id, input: {} })}
          >
            {tool.name}
          </button>
        ))}

        {found.length === 0 && <p className="palette__empty">Ничего не нашлось.</p>}
      </div>
    </div>
  )
}

function Node({
  node,
  label,
  selected,
  dragging,
  onSelect,
  onDragStart,
}: {
  node: CanvasNode
  label: string
  selected: boolean
  dragging: boolean
  onSelect: () => void
  onDragStart: () => void
}) {
  return (
    <button
      type="button"
      className="canvas__node"
      data-kind={node.kind}
      data-selected={selected ? 'true' : undefined}
      data-dragging={dragging ? 'true' : undefined}
      style={{ left: node.x, top: node.y, width: node.width, height: node.height }}
      onClick={onSelect}
      onPointerDown={(event) => {
        // Только левая кнопка и только с зажатой тягой: одиночный щелчок
        // должен выбирать, а не начинать перенос.
        if (event.button !== 0) return
        const start = { x: event.clientX, y: event.clientY }

        const move = (moved: PointerEvent) => {
          const far =
            Math.abs(moved.clientX - start.x) > 4 || Math.abs(moved.clientY - start.y) > 4
          if (far) {
            onDragStart()
            stop()
          }
        }

        const stop = () => {
          window.removeEventListener('pointermove', move)
          window.removeEventListener('pointerup', stop)
        }

        window.addEventListener('pointermove', move)
        window.addEventListener('pointerup', stop)
      }}
    >
      <span className="canvas__node-kind">{KIND_LABEL[node.kind]}</span>
      <span className="canvas__node-label">{label}</span>
    </button>
  )
}

const KIND_LABEL: Record<CanvasNode['kind'], string> = {
  trigger: 'Запуск',
  action: 'Действие',
  delay: 'Пауза',
  if: 'Условие',
}

/** Подпись узла: то, что человек ожидает прочитать, а не идентификатор. */
function stepLabel(step: Step | undefined, tools: ToolOption[]): string {
  if (!step) return 'шага нет'

  switch (step.kind) {
    case 'action': {
      const tool = tools.find((candidate) => candidate.id === step.toolId)
      return tool ? tool.name : `${step.toolId} — недоступен`
    }
    case 'delay':
      return step.ms >= 1000 ? `${(step.ms / 1000).toFixed(1)} с` : `${step.ms} мс`
    case 'if': {
      const { left, op, right } = step.condition
      if (!left) return 'условие не задано'
      if (op === 'empty') return `${left} пусто`
      if (op === 'not_empty') return `${left} не пусто`
      return `${left} ${OP_LABEL[op]} ${right ?? ''}`.trim()
    }
  }
}

const OP_LABEL: Record<string, string> = {
  contains: 'содержит',
  not_contains: 'не содержит',
  equals: '=',
  not_equals: '≠',
}

/** Плавная связь сверху вниз: прямая линия на ветвлении читается хуже. */
function curve(from: { x: number; y: number }, to: { x: number; y: number }): string {
  const bend = Math.max(16, Math.abs(to.y - from.y) / 2)
  return `M ${from.x} ${from.y} C ${from.x} ${from.y + bend}, ${to.x} ${to.y - bend}, ${to.x} ${to.y}`
}

function same(a: StepPath, b: StepPath | null): boolean {
  if (!b) return false
  return a.length === b.length && a.every((part, index) => part === b[index])
}
