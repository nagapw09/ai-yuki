/**
 * Правка дерева шагов по пути (ТЗ §16).
 *
 * # Зачем пути, а не индексы
 *
 * Шаги — это дерево, а не список: ветвление `if` держит внутри себя два своих
 * списка. Пока редактор был линейным, хватало индекса в верхнем списке; полотно
 * показывает и вложенные шаги, и «третий шаг внутри ветки „ложь“ второго шага»
 * индексом не назвать.
 *
 * # Почему всё возвращает новое дерево
 *
 * Шаги в модели помечены `readonly` не для красоты: команда, изменённая на
 * месте, разошлась бы с тем, что уже показано на экране, и React не узнал бы об
 * этом. Каждая правка собирает новый список и оставляет нетронутые ветки теми
 * же объектами — так сравнение по ссылке продолжает работать там, где ничего не
 * менялось.
 */

import type { Step } from './types'

/**
 * Путь к шагу внутри дерева.
 *
 * Число — позиция в списке, `'then'` и `'otherwise'` — выбор ветки у шага `if`.
 * `[2, 'otherwise', 0]` читается как «нулевой шаг ветки „иначе“ второго шага».
 */
export type StepPath = readonly (number | 'then' | 'otherwise')[]

/** Ветка шага `if`. */
export type Branch = 'then' | 'otherwise'

/** Шаг по пути или `undefined`, если пути в дереве нет. */
export function stepAt(steps: readonly Step[], path: StepPath): Step | undefined {
  if (path.length === 0) return undefined

  const [head, ...rest] = path
  if (typeof head !== 'number') return undefined

  const step = steps[head]
  if (!step) return undefined
  if (rest.length === 0) return step

  const [branch, ...tail] = rest
  if (step.kind !== 'if' || (branch !== 'then' && branch !== 'otherwise')) return undefined

  return stepAt(step[branch] ?? [], tail)
}

/**
 * Список, в котором лежит шаг по этому пути, и его позиция в нём.
 *
 * Нужен и для вставки, и для удаления: обе операции работают с родительским
 * списком, а не с самим шагом.
 */
function split(path: StepPath): { parent: StepPath; index: number } | null {
  if (path.length === 0) return null
  const index = path[path.length - 1]
  if (typeof index !== 'number') return null
  return { parent: path.slice(0, -1), index }
}

/** Список шагов по пути до списка (пустой путь — корневой список). */
export function listAt(steps: readonly Step[], path: StepPath): readonly Step[] | undefined {
  if (path.length === 0) return steps

  const last = path[path.length - 1]
  if (last !== 'then' && last !== 'otherwise') return undefined

  const owner = stepAt(steps, path.slice(0, -1))
  if (owner?.kind !== 'if') return undefined

  return owner[last] ?? []
}

/** Заменяет список шагов по пути, собирая дерево заново. */
function withList(
  steps: readonly Step[],
  path: StepPath,
  change: (list: readonly Step[]) => readonly Step[],
): readonly Step[] {
  if (path.length === 0) return change(steps)

  const last = path[path.length - 1]
  if (last !== 'then' && last !== 'otherwise') return steps

  const ownerPath = path.slice(0, -1)
  const parts = split(ownerPath)
  if (!parts) return steps

  return withList(steps, parts.parent, (list) => {
    const owner = list[parts.index]
    if (owner?.kind !== 'if') return list

    const next = [...list]
    next[parts.index] = { ...owner, [last]: change(owner[last] ?? []) }
    return next
  })
}

/** Вставляет шаг в список по пути, на позицию `index`. */
export function insertStep(
  steps: readonly Step[],
  list: StepPath,
  index: number,
  step: Step,
): readonly Step[] {
  return withList(steps, list, (current) => {
    // Позиция зажимается, а не отвергается: «в конец» удобно задавать длиной
    // списка, а перетаскивание за последний узел даёт индекс на единицу больше.
    const at = Math.max(0, Math.min(index, current.length))
    return [...current.slice(0, at), step, ...current.slice(at)]
  })
}

/** Убирает шаг по пути. */
export function removeStep(steps: readonly Step[], path: StepPath): readonly Step[] {
  const parts = split(path)
  if (!parts) return steps

  return withList(steps, parts.parent, (current) =>
    // Шага с таким номером нет — список возвращается тот же, а не его копия:
    // новая копия заставила бы React перерисовать ветку, в которой ничего не
    // изменилось.
    current[parts.index] === undefined
      ? current
      : current.filter((_, index) => index !== parts.index),
  )
}

/** Заменяет шаг по пути. */
export function replaceStep(
  steps: readonly Step[],
  path: StepPath,
  step: Step,
): readonly Step[] {
  const parts = split(path)
  if (!parts) return steps

  return withList(steps, parts.parent, (current) => {
    if (!current[parts.index]) return current
    const next = [...current]
    next[parts.index] = step
    return next
  })
}

/**
 * Лежит ли `inner` внутри `outer`.
 *
 * Проверка перед переносом: шаг нельзя положить внутрь себя самого. Без неё
 * ветвление, перетащенное в собственную ветку, исчезло бы из дерева вместе со
 * всем содержимым — удаление прошло бы, а вставлять было бы уже некуда.
 */
export function isInside(outer: StepPath, inner: StepPath): boolean {
  if (inner.length < outer.length) return false
  return outer.every((part, index) => inner[index] === part)
}

/**
 * Поправляет путь после удаления шага.
 *
 * Удаление сдвигает соседей по тому же списку — и не только соседей самого
 * шага: путь `[1, 'then']` после удаления шага `[0]` указывает уже на `[0,
 * 'then']`. Без этой поправки перенос «первого шага внутрь ветки второго»
 * кладёт его не туда, куда просили, а иногда и вовсе не кладёт.
 */
function shiftAfterRemoval(target: StepPath, from: StepPath): StepPath {
  const parts = split(from)
  if (!parts) return target

  const { parent, index } = parts
  if (target.length <= parent.length) return target

  // Сдвиг касается только путей, проходящих через тот же список.
  for (let level = 0; level < parent.length; level += 1) {
    if (target[level] !== parent[level]) return target
  }

  const at = target[parent.length]
  if (typeof at !== 'number' || at <= index) return target

  const next = [...target]
  next[parent.length] = at - 1
  return next
}

/**
 * Переносит шаг в другое место дерева.
 *
 * `target` — список, куда кладём, `index` — позиция в нём. Возвращает исходное
 * дерево, если перенос невозможен: внутрь себя, или пути не существует.
 *
 * Порядок важен: сначала удаляем, потом вставляем, и после удаления и путь до
 * списка, и позиция в нём могут съехать на единицу. Первое чинит
 * `shiftAfterRemoval`, второе — поправка индекса ниже. Обе ошибки в
 * перетаскивании замечают последними: перенос выглядит работающим, пока не
 * потащишь шаг сверху вниз или внутрь соседа.
 */
export function moveStep(
  steps: readonly Step[],
  from: StepPath,
  target: StepPath,
  index: number,
): readonly Step[] {
  const step = stepAt(steps, from)
  if (!step) return steps

  // Внутрь себя — нельзя. Сравниваем с путём до списка: `[1,'then']` лежит
  // внутри `[1]`.
  if (isInside(from, target)) return steps

  const parts = split(from)
  if (!parts) return steps

  const sameList =
    parts.parent.length === target.length &&
    parts.parent.every((part, i) => target[i] === part)

  const shifted = sameList && index > parts.index ? index - 1 : index

  return insertStep(
    removeStep(steps, from),
    shiftAfterRemoval(target, from),
    shifted,
    step,
  )
}
