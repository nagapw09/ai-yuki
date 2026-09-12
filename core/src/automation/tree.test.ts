import { describe, expect, it } from 'vitest'

import {
  insertStep,
  isInside,
  listAt,
  moveStep,
  removeStep,
  replaceStep,
  stepAt,
} from './tree'
import type { Step } from './types'

/** Шаг-действие с узнаваемым именем инструмента. */
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

/**
 * Дерево для проверок:
 *
 * 0 открыть
 * 1 если
 *   then:  0 напечатать
 *          1 если
 *            then: 0 нажать
 *   otherwise: 0 уведомить
 * 2 закрыть
 */
const tree: readonly Step[] = [
  action('open_app'),
  branching(
    [action('type_text'), branching([action('press_key')])],
    [action('notify')],
  ),
  action('close_app'),
]

function ids(steps: readonly Step[]): string[] {
  return steps.map((step) => (step.kind === 'action' ? step.toolId : step.kind))
}

describe('путь к шагу', () => {
  it('находит шаг верхнего уровня', () => {
    expect(stepAt(tree, [0])).toMatchObject({ toolId: 'open_app' })
    expect(stepAt(tree, [2])).toMatchObject({ toolId: 'close_app' })
  })

  it('находит шаг внутри ветки', () => {
    expect(stepAt(tree, [1, 'then', 0])).toMatchObject({ toolId: 'type_text' })
    expect(stepAt(tree, [1, 'otherwise', 0])).toMatchObject({ toolId: 'notify' })
    expect(stepAt(tree, [1, 'then', 1, 'then', 0])).toMatchObject({ toolId: 'press_key' })
  })

  it('молчит про несуществующий путь', () => {
    expect(stepAt(tree, [])).toBeUndefined()
    expect(stepAt(tree, [9])).toBeUndefined()
    expect(stepAt(tree, [0, 'then', 0])).toBeUndefined() // не ветвление
    expect(stepAt(tree, [1, 'otherwise', 5])).toBeUndefined()
    expect(stepAt(tree, ['then'])).toBeUndefined()
  })

  it('находит список по пути', () => {
    expect(ids(listAt(tree, [])!)).toEqual(['open_app', 'if', 'close_app'])
    expect(ids(listAt(tree, [1, 'then'])!)).toEqual(['type_text', 'if'])
    expect(ids(listAt(tree, [1, 'otherwise'])!)).toEqual(['notify'])
    expect(listAt(tree, [0, 'then'])).toBeUndefined()
  })
})

describe('правка дерева', () => {
  it('вставляет в корневой список', () => {
    expect(ids(insertStep(tree, [], 1, action('new')))).toEqual([
      'open_app',
      'new',
      'if',
      'close_app',
    ])
  })

  it('вставляет в ветку', () => {
    const next = insertStep(tree, [1, 'otherwise'], 0, action('new'))
    expect(ids(listAt(next, [1, 'otherwise'])!)).toEqual(['new', 'notify'])
    // Остальное дерево не тронуто.
    expect(ids(listAt(next, [1, 'then'])!)).toEqual(['type_text', 'if'])
  })

  it('зажимает позицию вставки вместо отказа', () => {
    expect(ids(insertStep(tree, [], 99, action('new')))).toEqual([
      'open_app',
      'if',
      'close_app',
      'new',
    ])
    expect(ids(insertStep(tree, [], -5, action('new')))).toEqual([
      'new',
      'open_app',
      'if',
      'close_app',
    ])
  })

  it('вставляет в ветку, которой ещё нет', () => {
    // У этого `if` ветки «иначе» нет вовсе, и это нормальное состояние.
    const next = insertStep(tree, [1, 'then', 1, 'otherwise'], 0, action('new'))
    expect(ids(listAt(next, [1, 'then', 1, 'otherwise'])!)).toEqual(['new'])
  })

  it('удаляет шаг', () => {
    expect(ids(removeStep(tree, [1]))).toEqual(['open_app', 'close_app'])

    const inner = removeStep(tree, [1, 'then', 0])
    expect(ids(listAt(inner, [1, 'then'])!)).toEqual(['if'])
  })

  it('заменяет шаг', () => {
    const next = replaceStep(tree, [1, 'then', 0], action('changed'))
    expect(ids(listAt(next, [1, 'then'])!)).toEqual(['changed', 'if'])
  })

  it('не портит дерево на несуществующем пути', () => {
    // Ровно тот же список, а не его копия: перерисовывать ветку, в которой
    // ничего не изменилось, незачем.
    expect(removeStep(tree, [9])).toBe(tree)
    expect(removeStep(tree, [])).toBe(tree)
    expect(replaceStep(tree, [9], action('new'))).toBe(tree)
  })

  it('оставляет нетронутые ветки теми же объектами', () => {
    const next = insertStep(tree, [1, 'otherwise'], 0, action('new'))
    // Первый и третий шаги не менялись — React не должен считать их новыми.
    expect(next[0]).toBe(tree[0])
    expect(next[2]).toBe(tree[2])
  })
})

describe('перенос шага', () => {
  it('переносит внутрь ветки', () => {
    const next = moveStep(tree, [0], [1, 'then'], 0)

    // Ветвление было вторым, а после удаления первого шага стало первым —
    // и путь к его ветке съехал вместе с ним.
    expect(ids(next)).toEqual(['if', 'close_app'])
    expect(ids(listAt(next, [0, 'then'])!)).toEqual(['open_app', 'type_text', 'if'])
  })

  it('переносит из ветки наружу', () => {
    const next = moveStep(tree, [1, 'otherwise', 0], [], 0)
    expect(ids(next)).toEqual(['notify', 'open_app', 'if', 'close_app'])
    expect(listAt(next, [2, 'otherwise'])).toEqual([])
  })

  // Самая незаметная ошибка перетаскивания: при переносе вниз в том же списке
  // шаг сначала удаляется, и все позиции после него сдвигаются.
  it('не уезжает на позицию при переносе вниз в том же списке', () => {
    const list: readonly Step[] = [action('a'), action('b'), action('c')]

    // «Поставить a между b и c» — это позиция 2 в исходной нумерации.
    expect(ids(moveStep(list, [0], [], 2))).toEqual(['b', 'a', 'c'])
    // «Поставить a в самый конец».
    expect(ids(moveStep(list, [0], [], 3))).toEqual(['b', 'c', 'a'])
    // Перенос вверх сдвига не требует.
    expect(ids(moveStep(list, [2], [], 0))).toEqual(['c', 'a', 'b'])
  })

  it('никуда не переносит шаг внутрь себя самого', () => {
    // Ветвление в собственную ветку: удаление прошло бы, а вставлять было бы
    // уже некуда, и весь шаг с содержимым исчез бы.
    expect(moveStep(tree, [1], [1, 'then'], 0)).toBe(tree)
    expect(moveStep(tree, [1], [1, 'otherwise'], 0)).toBe(tree)
    expect(moveStep(tree, [1, 'then', 1], [1, 'then', 1, 'then'], 0)).toBe(tree)
  })

  it('не двигает то, чего нет', () => {
    expect(moveStep(tree, [9], [], 0)).toBe(tree)
  })

  it('перенос на то же место ничего не меняет', () => {
    expect(ids(moveStep(tree, [0], [], 0))).toEqual(ids(tree))
  })
})

describe('вложенность путей', () => {
  it('видит, что путь лежит внутри другого', () => {
    expect(isInside([1], [1, 'then'])).toBe(true)
    expect(isInside([1], [1, 'then', 0])).toBe(true)
    expect(isInside([1], [1])).toBe(true)
  })

  it('видит, что не лежит', () => {
    expect(isInside([1], [0, 'then'])).toBe(false)
    expect(isInside([1, 'then'], [1])).toBe(false)
    expect(isInside([1, 'then'], [1, 'otherwise'])).toBe(false)
  })
})
