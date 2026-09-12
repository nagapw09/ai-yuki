/**
 * Раскладка команды на полотне (ТЗ §16, ТЗ §39 Visual Command Builder).
 *
 * # Почему раскладка считается, а не задаётся мышью
 *
 * Команда — это дерево шагов, а не произвольный граф: движок выполняет список
 * сверху вниз и заходит в одну из двух ветвей `if`. Дать человеку расставлять
 * узлы и тянуть связи куда угодно значило бы позволить нарисовать то, что
 * выполнить нельзя, — и объяснять потом, почему красивая схема не запускается.
 *
 * Поэтому положение узлов выводится из самого дерева. Перетаскивание при этом
 * остаётся: тащат не узел в пустоту, а шаг в другое место дерева, и полотно
 * перекладывается само.
 *
 * # Почему это отдельный модуль без React
 *
 * Здесь одна арифметика, и её можно проверить тестами: где узлы, куда идут
 * связи, не наезжают ли ветки друг на друга. В компоненте та же арифметика
 * проверялась бы только глазами на каждой сборке.
 */

import type { Step, Trigger } from './types'
import { listAt, type StepPath } from './tree'

/** Размеры узла и зазоры, в пикселях полотна. */
export const NODE_WIDTH = 208
export const NODE_HEIGHT = 60

/** Вертикальный зазор между узлами: в нём живут связи и места для вставки. */
export const GAP_Y = 52

/** Горизонтальный зазор между ветками `если`. */
export const GAP_X = 40

/** Высота метки «положить сюда» в пустой ветке. */
export const SLOT_HEIGHT = 32

/** Отступ полотна от краёв. */
export const PADDING = 40

export interface Point {
  x: number
  y: number
}

/** Узел на полотне. */
export interface CanvasNode {
  /**
   * Путь к шагу. У триггера путь пустой: он не шаг, а причина запуска, и
   * редактируется отдельно.
   */
  path: StepPath
  kind: 'trigger' | 'action' | 'delay' | 'if'
  /** Левый верхний угол. */
  x: number
  y: number
  width: number
  height: number
}

/** Связь между узлами. */
export interface CanvasEdge {
  from: Point
  to: Point
  /** «Истина» или «Ложь» на выходе из ветвления. */
  label?: string
}

/** Место, куда можно положить шаг. */
export interface CanvasSlot {
  /** Список, в который кладём. */
  list: StepPath
  /** Позиция в этом списке. */
  index: number
  /** Центр метки. */
  x: number
  y: number
}

export interface Canvas {
  nodes: CanvasNode[]
  edges: CanvasEdge[]
  slots: CanvasSlot[]
  width: number
  height: number
}

/** Габариты поддерева. */
interface Size {
  width: number
  height: number
}

/** Во что превращается шаг при раскладке. */
function kindOf(step: Step): CanvasNode['kind'] {
  return step.kind
}

/**
 * Габариты списка шагов.
 *
 * Пустой список занимает место под метку вставки, а не ноль: ветка `если` без
 * шагов должна быть видна и должна принимать перетаскивание, иначе положить в
 * неё первый шаг было бы некуда.
 */
function measure(list: readonly Step[]): Size {
  if (list.length === 0) {
    return { width: NODE_WIDTH, height: SLOT_HEIGHT }
  }

  let height = 0
  let width = NODE_WIDTH

  list.forEach((step, index) => {
    if (index > 0) height += GAP_Y

    if (step.kind === 'if') {
      const then = measure(step.then)
      const otherwise = measure(step.otherwise ?? [])

      height += NODE_HEIGHT + GAP_Y + Math.max(then.height, otherwise.height)
      width = Math.max(width, then.width + GAP_X + otherwise.width)
    } else {
      height += NODE_HEIGHT
    }
  })

  return { width, height }
}

/** Результат раскладки одного списка. */
interface Placed {
  /** Куда входит связь сверху. */
  entry: Point
  /** Откуда выходят связи вниз; у ветвления их две или больше. */
  exits: Point[]
  /** Нижняя граница занятого места. */
  bottom: number
}

/**
 * Раскладывает список шагов, центрируя его по `centerX`.
 *
 * Возвращает точки входа и выхода: по ним связи сшиваются между уровнями, и
 * благодаря выходам ветки `если` сходятся к следующему шагу сами — без
 * отдельного «узла соединения», которого в модели нет.
 */
function place(
  list: readonly Step[],
  path: StepPath,
  centerX: number,
  top: number,
  out: Canvas,
): Placed {
  if (list.length === 0) {
    out.slots.push({ list: path, index: 0, x: centerX, y: top + SLOT_HEIGHT / 2 })
    return {
      entry: { x: centerX, y: top },
      exits: [{ x: centerX, y: top + SLOT_HEIGHT }],
      bottom: top + SLOT_HEIGHT,
    }
  }

  let y = top
  let entry: Point | null = null
  let pending: Point[] = []

  list.forEach((step, index) => {
    const nodePath: StepPath = [...path, index]
    const nodeEntry = { x: centerX, y }

    if (index === 0) {
      entry = nodeEntry
    } else {
      // Метка вставки живёт в зазоре над узлом: так «между вторым и третьим»
      // указывается ровно там, где человек это и видит.
      out.slots.push({ list: path, index, x: centerX, y: y - GAP_Y / 2 })
    }

    for (const exit of pending) {
      out.edges.push({ from: exit, to: nodeEntry })
    }

    out.nodes.push({
      path: nodePath,
      kind: kindOf(step),
      x: centerX - NODE_WIDTH / 2,
      y,
      width: NODE_WIDTH,
      height: NODE_HEIGHT,
    })

    if (step.kind === 'if') {
      const branchTop = y + NODE_HEIGHT + GAP_Y
      const then = measure(step.then)
      const otherwise = measure(step.otherwise ?? [])
      const total = then.width + GAP_X + otherwise.width

      const thenCenter = centerX - total / 2 + then.width / 2
      const elseCenter = centerX + total / 2 - otherwise.width / 2

      const placedThen = place(step.then, [...nodePath, 'then'], thenCenter, branchTop, out)
      const placedElse = place(
        step.otherwise ?? [],
        [...nodePath, 'otherwise'],
        elseCenter,
        branchTop,
        out,
      )

      const bottom = { x: centerX, y: y + NODE_HEIGHT }
      out.edges.push({ from: bottom, to: placedThen.entry, label: 'Истина' })
      out.edges.push({ from: bottom, to: placedElse.entry, label: 'Ложь' })

      pending = [...placedThen.exits, ...placedElse.exits]
      y = Math.max(placedThen.bottom, placedElse.bottom)
    } else {
      pending = [{ x: centerX, y: y + NODE_HEIGHT }]
      y += NODE_HEIGHT
    }

    if (index < list.length - 1) y += GAP_Y
  })

  // Место для вставки в конец — ниже последнего узла.
  out.slots.push({ list: path, index: list.length, x: centerX, y: y + GAP_Y / 2 })

  return {
    entry: entry ?? { x: centerX, y: top },
    exits: pending,
    bottom: y,
  }
}

/** Человеческое имя триггера — оно же подпись верхнего узла. */
export function triggerLabel(trigger: Trigger): string {
  switch (trigger.kind) {
    case 'phrase':
      return trigger.phrase ? `Фраза: «${trigger.phrase}»` : 'Фраза не задана'
    case 'hotkey':
      return trigger.shortcut ? `Сочетание: ${trigger.shortcut}` : 'Сочетание не задано'
    case 'startup':
      return 'При запуске Yuki'
    case 'manual':
      return 'Только вручную'
  }
}

/**
 * Считает полотно для команды.
 *
 * Триггер сверху, шаги под ним. Всё смещено на отступ, чтобы верхний узел не
 * упирался в край области прокрутки.
 */
export function layoutCommand(steps: readonly Step[]): Canvas {
  const out: Canvas = { nodes: [], edges: [], slots: [], width: 0, height: 0 }

  const size = measure(steps)
  const centerX = PADDING + size.width / 2

  const trigger: CanvasNode = {
    path: [],
    kind: 'trigger',
    x: centerX - NODE_WIDTH / 2,
    y: PADDING,
    width: NODE_WIDTH,
    height: NODE_HEIGHT,
  }
  out.nodes.push(trigger)

  const placed = place(steps, [], centerX, PADDING + NODE_HEIGHT + GAP_Y, out)

  out.edges.push({
    from: { x: centerX, y: PADDING + NODE_HEIGHT },
    to: placed.entry,
  })

  // Первое место вставки — между триггером и первым шагом.
  out.slots.push({
    list: [],
    index: 0,
    x: centerX,
    y: PADDING + NODE_HEIGHT + GAP_Y / 2,
  })

  out.width = size.width + PADDING * 2
  out.height = placed.bottom + PADDING + GAP_Y

  return out
}

/** Узел, стоящий под точкой; `null`, если под ней пусто. */
export function nodeAt(canvas: Canvas, point: Point): CanvasNode | null {
  // Обратный порядок: вложенные узлы добавляются позже и лежат «сверху».
  for (let index = canvas.nodes.length - 1; index >= 0; index -= 1) {
    const node = canvas.nodes[index]
    if (!node) continue

    if (
      point.x >= node.x &&
      point.x <= node.x + node.width &&
      point.y >= node.y &&
      point.y <= node.y + node.height
    ) {
      return node
    }
  }

  return null
}

/**
 * Ближайшее место вставки к точке, но не дальше `radius`.
 *
 * Радиус нужен, чтобы бросок в пустоту не «прилипал» к дальнему месту на другом
 * конце полотна: человек, отпустивший шаг мимо, ждёт, что ничего не произойдёт.
 */
export function slotNear(
  canvas: Canvas,
  point: Point,
  radius = NODE_HEIGHT * 1.5,
): CanvasSlot | null {
  let best: CanvasSlot | null = null
  let bestDistance = radius

  for (const slot of canvas.slots) {
    const dx = slot.x - point.x
    const dy = slot.y - point.y
    const distance = Math.sqrt(dx * dx + dy * dy)

    if (distance <= bestDistance) {
      best = slot
      bestDistance = distance
    }
  }

  return best
}

/** Сколько шагов в дереве, включая вложенные — для подписи «шагов: N». */
export function countSteps(steps: readonly Step[]): number {
  return steps.reduce((total, step) => {
    if (step.kind !== 'if') return total + 1
    return total + 1 + countSteps(step.then) + countSteps(step.otherwise ?? [])
  }, 0)
}

/** Существует ли такой список в дереве — проверка перед вставкой из палитры. */
export function hasList(steps: readonly Step[], path: StepPath): boolean {
  return listAt(steps, path) !== undefined
}
