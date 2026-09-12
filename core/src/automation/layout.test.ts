import { describe, expect, it } from 'vitest'

import {
  countSteps,
  layoutCommand,
  nodeAt,
  slotNear,
  triggerLabel,
  GAP_X,
  NODE_HEIGHT,
  NODE_WIDTH,
  type Canvas,
} from './layout'
import type { Step } from './types'

function action(toolId: string): Step {
  return { kind: 'action', toolId, input: {} }
}

function branching(then: readonly Step[], otherwise?: readonly Step[]): Step {
  return {
    kind: 'if',
    condition: { left: '{{x}}', op: 'not_empty' },
    then,
    ...(otherwise ? { otherwise } : {}),
  }
}

/** Узел по пути — по нему удобно проверять положение. */
function node(canvas: Canvas, path: readonly (number | 'then' | 'otherwise')[]) {
  const found = canvas.nodes.find(
    (candidate) =>
      candidate.path.length === path.length &&
      candidate.path.every((part, index) => part === path[index]),
  )
  if (!found) throw new Error(`узла ${JSON.stringify(path)} нет на полотне`)
  return found
}

/** Пересекаются ли два узла — ни один не должен наезжать на другой. */
function overlaps(a: Canvas['nodes'][number], b: Canvas['nodes'][number]): boolean {
  return (
    a.x < b.x + b.width &&
    b.x < a.x + a.width &&
    a.y < b.y + b.height &&
    b.y < a.y + a.height
  )
}

describe('раскладка полотна', () => {
  it('ставит триггер сверху, а шаги под ним', () => {
    const canvas = layoutCommand([action('open_app'), action('close_app')])

    const trigger = node(canvas, [])
    const first = node(canvas, [0])
    const second = node(canvas, [1])

    expect(trigger.y).toBeLessThan(first.y)
    expect(first.y).toBeLessThan(second.y)
    // Линейная команда идёт одной колонкой.
    expect(first.x).toBe(second.x)
    expect(trigger.x).toBe(first.x)
  })

  it('разводит ветки если по разные стороны', () => {
    const canvas = layoutCommand([
      branching([action('type_text')], [action('notify')]),
    ])

    const condition = node(canvas, [0])
    const then = node(canvas, [0, 'then', 0])
    const otherwise = node(canvas, [0, 'otherwise', 0])

    expect(then.x).toBeLessThan(condition.x)
    expect(otherwise.x).toBeGreaterThan(condition.x)
    // Обе ветки на одной высоте: они равноправны.
    expect(then.y).toBe(otherwise.y)
    expect(then.y).toBeGreaterThan(condition.y)
    // И не наезжают друг на друга.
    expect(otherwise.x - (then.x + then.width)).toBeGreaterThanOrEqual(GAP_X - 1)
  })

  it('ни один узел не наезжает на другой', () => {
    const canvas = layoutCommand([
      action('open_app'),
      branching(
        [action('type_text'), branching([action('press_key')], [action('notify')])],
        [action('close_app')],
      ),
      action('set_volume'),
    ])

    for (let i = 0; i < canvas.nodes.length; i += 1) {
      for (let j = i + 1; j < canvas.nodes.length; j += 1) {
        const a = canvas.nodes[i]!
        const b = canvas.nodes[j]!
        expect(overlaps(a, b), `${JSON.stringify(a.path)} и ${JSON.stringify(b.path)}`).toBe(
          false,
        )
      }
    }
  })

  it('сшивает обе ветки со следующим шагом', () => {
    const canvas = layoutCommand([
      branching([action('type_text')], [action('notify')]),
      action('close_app'),
    ])

    const next = node(canvas, [1])
    const incoming = canvas.edges.filter(
      (edge) => edge.to.x === next.x + next.width / 2 && edge.to.y === next.y,
    )

    // Из каждой ветки — своя связь: после ветвления выполнение продолжается
    // с того же шага, каким бы путём ни пошло.
    expect(incoming).toHaveLength(2)
  })

  it('ведёт связь от самого ветвления, если ветка пуста', () => {
    const canvas = layoutCommand([branching([action('type_text')]), action('close_app')])

    const next = node(canvas, [1])
    const incoming = canvas.edges.filter(
      (edge) => edge.to.x === next.x + next.width / 2 && edge.to.y === next.y,
    )

    // Одна из «ветки» — пустая, и её выход это место вставки под условием.
    expect(incoming).toHaveLength(2)
  })

  it('подписывает выходы ветвления', () => {
    const canvas = layoutCommand([branching([action('a')], [action('b')])])
    const labels = canvas.edges.map((edge) => edge.label).filter(Boolean)

    expect(labels).toContain('Истина')
    expect(labels).toContain('Ложь')
  })

  it('даёт место для вставки в пустую ветку', () => {
    const canvas = layoutCommand([branching([], [])])

    const intoThen = canvas.slots.filter(
      (slot) => slot.list.length === 2 && slot.list[1] === 'then',
    )
    const intoElse = canvas.slots.filter(
      (slot) => slot.list.length === 2 && slot.list[1] === 'otherwise',
    )

    expect(intoThen.length).toBeGreaterThan(0)
    expect(intoElse.length).toBeGreaterThan(0)
  })

  it('даёт места вставки до, между и после шагов', () => {
    const canvas = layoutCommand([action('a'), action('b')])

    const root = canvas.slots.filter((slot) => slot.list.length === 0)
    const indexes = [...new Set(root.map((slot) => slot.index))].sort()

    expect(indexes).toEqual([0, 1, 2])
  })

  it('у пустой команды есть куда положить первый шаг', () => {
    const canvas = layoutCommand([])

    expect(canvas.nodes).toHaveLength(1) // только триггер
    expect(canvas.slots.some((slot) => slot.list.length === 0 && slot.index === 0)).toBe(true)
    expect(canvas.height).toBeGreaterThan(NODE_HEIGHT)
  })

  it('полотно вмещает все узлы', () => {
    const canvas = layoutCommand([
      branching(
        [action('a'), branching([action('b')], [action('c')])],
        [action('d')],
      ),
    ])

    for (const item of canvas.nodes) {
      expect(item.x).toBeGreaterThanOrEqual(0)
      expect(item.y).toBeGreaterThanOrEqual(0)
      expect(item.x + item.width).toBeLessThanOrEqual(canvas.width)
      expect(item.y + item.height).toBeLessThanOrEqual(canvas.height)
    }
  })
})

describe('попадание по полотну', () => {
  const canvas = layoutCommand([action('open_app'), action('close_app')])

  it('находит узел под точкой', () => {
    const first = node(canvas, [0])
    const hit = nodeAt(canvas, { x: first.x + 10, y: first.y + 10 })

    expect(hit?.path).toEqual([0])
  })

  it('молчит, когда под точкой пусто', () => {
    expect(nodeAt(canvas, { x: 5, y: 5 })).toBeNull()
    expect(nodeAt(canvas, { x: 5000, y: 5000 })).toBeNull()
  })

  it('вложенный узел важнее внешнего', () => {
    // Узлы ветвления добавляются после самого ветвления, и попадание должно
    // достаться тому, что нарисован сверху.
    const nested = layoutCommand([branching([action('inner')])])
    const inner = node(nested, [0, 'then', 0])
    const hit = nodeAt(nested, { x: inner.x + 4, y: inner.y + 4 })

    expect(hit?.path).toEqual([0, 'then', 0])
  })
})

describe('ближайшее место вставки', () => {
  const canvas = layoutCommand([action('a'), action('b')])

  it('находит место рядом с точкой', () => {
    const middle = canvas.slots.find((slot) => slot.list.length === 0 && slot.index === 1)!
    const found = slotNear(canvas, { x: middle.x + 6, y: middle.y + 6 })

    expect(found?.index).toBe(1)
  })

  it('не прилипает к далёкому месту', () => {
    expect(slotNear(canvas, { x: 4000, y: 4000 })).toBeNull()
  })
})

describe('подписи и счёт', () => {
  it('называет триггер по-человечески', () => {
    expect(triggerLabel({ kind: 'phrase', phrase: 'рабочий режим' })).toBe(
      'Фраза: «рабочий режим»',
    )
    expect(triggerLabel({ kind: 'phrase', phrase: '' })).toBe('Фраза не задана')
    expect(triggerLabel({ kind: 'hotkey', shortcut: 'Ctrl+Alt+W' })).toBe(
      'Сочетание: Ctrl+Alt+W',
    )
    expect(triggerLabel({ kind: 'hotkey', shortcut: '' })).toBe('Сочетание не задано')
    expect(triggerLabel({ kind: 'startup' })).toBe('При запуске Yuki')
    expect(triggerLabel({ kind: 'manual' })).toBe('Только вручную')
  })

  it('считает шаги вместе с вложенными', () => {
    expect(countSteps([])).toBe(0)
    expect(countSteps([action('a')])).toBe(1)
    expect(
      countSteps([branching([action('a'), action('b')], [action('c')]), action('d')]),
    ).toBe(5)
  })

  it('ширина узла одинакова у всех', () => {
    const canvas = layoutCommand([branching([action('a')], [action('b')])])
    for (const item of canvas.nodes) {
      expect(item.width).toBe(NODE_WIDTH)
    }
  })
})
