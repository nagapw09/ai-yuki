/**
 * Исполнитель автоматизаций (ТЗ §16).
 *
 * Шаги выполняются последовательно, через тот же реестр инструментов и тот же
 * Permission Gate, что и действия модели. Это не экономия кода: команда,
 * обходящая проверку разрешений, была бы дырой ровно того размера, который
 * закрывает ТЗ §21 — записал один раз, выполняется без вопросов навсегда.
 */

import type { GateDecision } from '../permissions/gate'
import type { Tool } from '../agent/types'
import type { ToolRegistry } from '../agent/registry'
import type { Command, Condition, RunEvent, RunOutcome, Step } from './types'

/**
 * Потолок числа шагов за один запуск.
 *
 * Ветвления позволяют построить цикл через вложенность, и без потолка ошибка
 * в команде выражается не в сообщении, а в зависшем приложении.
 */
const MAX_STEPS = 200

export interface RunDeps {
  readonly registry: ToolRegistry
  /** Проверка разрешений и риска (ТЗ §21, §22). */
  readonly decide: (tool: Tool) => GateDecision
  /** Подтверждение опасного шага; те же правила, что и у модели (ТЗ §22). */
  readonly confirm: (tool: Tool, input: unknown) => Promise<boolean>
  readonly onEvent?: (event: RunEvent) => void
  readonly log?: (entry: {
    tool: string
    status: 'ok' | 'error' | 'cancelled' | 'denied'
    result?: string
    durationMs: number
  }) => void
  /** Пауза между шагами; подменяется в тестах. */
  readonly sleep?: (ms: number) => Promise<void>
  readonly signal?: AbortSignal
}

/**
 * Подставляет `{{имя}}` значениями, сохранёнными предыдущими шагами.
 *
 * Шаблон юникодный намеренно: `\w` в JavaScript — это только латиница, а имена
 * переменных пишет человек, и по-русски он их называет ровно так же охотно,
 * как по-английски.
 */
export function interpolate(value: string, variables: Record<string, string>): string {
  return value.replace(/\{\{\s*([\p{L}\p{N}_.-]+)\s*\}\}/gu, (match, name: string) =>
    // Неизвестное имя оставляем как есть: молча подставленная пустота
    // превращает «открой {{файл}}» в «открой», и понять это уже нельзя.
    name in variables ? variables[name] ?? '' : match,
  )
}

/** Проходит по объекту аргументов и подставляет переменные в строки. */
function resolveInput(
  input: Record<string, unknown>,
  variables: Record<string, string>,
): Record<string, unknown> {
  const walk = (value: unknown): unknown => {
    if (typeof value === 'string') return interpolate(value, variables)
    if (Array.isArray(value)) return value.map(walk)
    if (value && typeof value === 'object') {
      return Object.fromEntries(
        Object.entries(value as Record<string, unknown>).map(([k, v]) => [k, walk(v)]),
      )
    }
    return value
  }

  return walk(input) as Record<string, unknown>
}

/** Вычисляет условие ветвления. */
export function evaluateCondition(
  condition: Condition,
  variables: Record<string, string>,
): boolean {
  const left = interpolate(condition.left, variables)
  const right = interpolate(condition.right ?? '', variables)

  switch (condition.op) {
    case 'contains':
      return left.includes(right)
    case 'not_contains':
      return !left.includes(right)
    case 'equals':
      return left === right
    case 'not_equals':
      return left !== right
    case 'empty':
      return left.trim() === ''
    case 'not_empty':
      return left.trim() !== ''
  }
}

function describe(value: unknown): string {
  if (value === undefined || value === null) return 'готово'
  if (typeof value === 'string') return value
  const json = JSON.stringify(value)
  return json.length > 400 ? `${json.slice(0, 400)}…` : json
}

function label(step: Step, registry: ToolRegistry): string {
  switch (step.kind) {
    case 'action':
      return registry.get(step.toolId)?.name ?? step.toolId
    case 'delay':
      return `пауза ${step.ms} мс`
    case 'if':
      return 'условие'
  }
}

/** Выполняет команду. */
export async function runCommand(
  command: Command,
  deps: RunDeps,
): Promise<RunOutcome> {
  const variables: Record<string, string> = {}
  const sleep = deps.sleep ?? ((ms: number) => new Promise((r) => setTimeout(r, ms)))
  let executed = 0

  const fail = (index: number, message: string): RunOutcome => {
    deps.onEvent?.({ kind: 'failed', index, message })
    return { ok: false, executed, variables, error: message }
  }

  /** Выполняет список шагов; возвращает ошибку, если она случилась. */
  const runSteps = async (steps: readonly Step[]): Promise<string | null> => {
    for (const [index, step] of steps.entries()) {
      if (deps.signal?.aborted) return 'Команда отменена'
      if (executed >= MAX_STEPS) {
        return `Команда превысила предел в ${MAX_STEPS} шагов — похоже на зацикливание`
      }

      deps.onEvent?.({ kind: 'step_started', index, label: label(step, deps.registry) })

      if (step.kind === 'delay') {
        executed += 1
        await sleep(Math.max(0, step.ms))
        deps.onEvent?.({ kind: 'step_finished', index, ok: true, detail: 'готово' })
        continue
      }

      if (step.kind === 'if') {
        executed += 1
        const taken = evaluateCondition(step.condition, variables)
        deps.onEvent?.({
          kind: 'step_finished',
          index,
          ok: true,
          detail: taken ? 'условие выполнено' : 'условие не выполнено',
        })
        const branch = taken ? step.then : step.otherwise ?? []
        const error = await runSteps(branch)
        if (error) return error
        continue
      }

      const tool = deps.registry.get(step.toolId)
      if (!tool) {
        // Инструмент мог исчезнуть вместе с выключенной возможностью.
        // Команда, молча пропустившая шаг, хуже команды, которая остановилась.
        return `В команде используется инструмент «${step.toolId}», которого больше нет`
      }

      const decision = deps.decide(tool)
      if (decision.kind === 'deny') {
        const reason =
          decision.reason === 'os_permission_missing'
            ? `нет системного разрешения «${decision.category}»`
            : `не разрешена категория «${decision.category}»`
        deps.log?.({ tool: tool.id, status: 'denied', result: reason, durationMs: 0 })
        return `Шаг «${tool.name}» не выполнен: ${reason}`
      }

      if (decision.kind === 'confirm') {
        const approved = await deps.confirm(tool, step.input)
        if (!approved) {
          deps.log?.({ tool: tool.id, status: 'cancelled', durationMs: 0 })
          deps.onEvent?.({ kind: 'skipped', index, reason: 'пользователь отказал' })
          return `Шаг «${tool.name}» отменён пользователем`
        }
      }

      const started = performance.now()
      try {
        const value = await tool.execute(resolveInput(step.input, variables), {
          signal: deps.signal ?? new AbortController().signal,
          report: () => undefined,
          // Картинки в командах не нужны: их некому смотреть — модель в
          // выполнении не участвует.
          attach: () => undefined,
        })
        const durationMs = Math.round(performance.now() - started)
        const detail = describe(value)

        executed += 1
        if (step.saveAs) variables[step.saveAs] = detail
        deps.log?.({ tool: tool.id, status: 'ok', result: detail, durationMs })
        deps.onEvent?.({ kind: 'step_finished', index, ok: true, detail })
      } catch (error) {
        const durationMs = Math.round(performance.now() - started)
        const message = error instanceof Error ? error.message : String(error)
        deps.log?.({ tool: tool.id, status: 'error', result: message, durationMs })
        deps.onEvent?.({ kind: 'step_finished', index, ok: false, detail: message })
        // Останавливаемся на первой ошибке: следующие шаги почти всегда
        // опираются на результат предыдущих, и продолжать — значит доламывать.
        return `Шаг «${tool.name}» не удался: ${message}`
      }
    }

    return null
  }

  const error = await runSteps(command.steps)
  if (error) return fail(executed, error)

  deps.onEvent?.({ kind: 'finished', steps: executed })
  return { ok: true, executed, variables }
}

/**
 * Подбирает команду по реплике пользователя (ТЗ §16: voice phrase).
 *
 * Совпадение по вхождению, а не по равенству: человек говорит «Юки, запусти
 * рабочий режим, пожалуйста», и требовать дословности значило бы, что команда
 * срабатывает через раз. Из подошедших берётся самая длинная фраза — она
 * конкретнее, и «рабочий режим» не перехватит «режим».
 */
export function matchCommand(
  text: string,
  commands: readonly Command[],
): Command | undefined {
  const normalized = text.trim().toLowerCase()
  if (!normalized) return undefined

  return commands
    .filter((c) => c.enabled && c.trigger.kind === 'phrase')
    .filter((c) => {
      const phrase = (c.trigger as { phrase: string }).phrase.trim().toLowerCase()
      return phrase.length > 0 && normalized.includes(phrase)
    })
    .sort(
      (a, b) =>
        (b.trigger as { phrase: string }).phrase.length -
        (a.trigger as { phrase: string }).phrase.length,
    )[0]
}
