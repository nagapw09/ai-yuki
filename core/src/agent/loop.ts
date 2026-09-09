/**
 * Agent Loop (ТЗ §5).
 *
 *   INPUT → UNDERSTAND → PLAN → TOOL SELECTION → EXECUTE
 *         → OBSERVE → VERIFY → NEXT STEP / COMPLETE → RESPONSE
 *
 * Понимание и планирование делает модель, а этот код отвечает за то, чего модель
 * сделать не может: выбрать инструмент из реестра, спросить разрешение, выполнить,
 * и — главное — вернуть модели **настоящий** результат. Именно здесь держится
 * инвариант ТЗ §5: модель не может отрапортовать об успехе, потому что успех она
 * узнаёт только из `tool_result`, который приходит отсюда.
 */

import type { GateDecision } from '../permissions/gate'
import type { ChatFn, ContentBlock, Message } from './protocol'
import { responseText, toolUses } from './protocol'
import type { ToolRegistry } from './registry'
import type { Tool, ToolError } from './types'

/**
 * Потолок числа обращений к модели за один запрос пользователя.
 *
 * Нужен не от «плохих» моделей, а от честных циклов: инструмент, который стабильно
 * возвращает ошибку, модель будет добросовестно пробовать снова. Без потолка это
 * тратит деньги пользователя молча.
 */
const DEFAULT_MAX_STEPS = 12

export type AgentEvent =
  /** Модель прислала текст (полностью, после стрима). */
  | { readonly kind: 'text'; readonly text: string }
  /** Инструмент начал выполняться — безопасный статус для UI (ТЗ §15). */
  | { readonly kind: 'tool_started'; readonly toolId: string; readonly summary: string }
  | {
      readonly kind: 'tool_finished'
      readonly toolId: string
      readonly ok: boolean
      readonly detail: string
      readonly durationMs: number
    }
  /** Действие отклонено политикой разрешений (ТЗ §21, §22). */
  | { readonly kind: 'blocked'; readonly toolId: string; readonly reason: string }
  | { readonly kind: 'done'; readonly steps: number }
  | { readonly kind: 'error'; readonly message: string }

/** Что вызывающая сторона решила делать с действием. */
export type Approval = 'allow' | 'deny'

export interface AgentDeps {
  readonly chat: ChatFn
  readonly registry: ToolRegistry
  /** Проверка разрешений и риска (ТЗ §21, §22). */
  readonly decide: (tool: Tool) => GateDecision
  /** Показ модалки подтверждения с планом действия (ТЗ §22). */
  readonly confirm: (tool: Tool, input: unknown) => Promise<Approval>
  readonly onEvent: (event: AgentEvent) => void
  /** Запись в журнал активности (ТЗ §23). */
  readonly log?: (entry: {
    tool: string
    status: 'ok' | 'error' | 'cancelled' | 'denied'
    target?: string
    result?: string
    durationMs: number
  }) => void
  readonly systemExtra?: string
  readonly maxSteps?: number
  readonly signal?: AbortSignal
}

export interface AgentOutcome {
  /** История, дополненная ходами этого запроса: её сохраняет вызывающая сторона. */
  readonly messages: Message[]
  /** Финальный текст для пользователя. */
  readonly reply: string
  readonly steps: number
}

/**
 * Формулировка отказа, когда возможности нет (ТЗ §42).
 *
 * Текст уходит модели как результат инструмента, а не пользователю напрямую:
 * модель должна встроить его в разговор и предложить добавить возможность,
 * а не оборвать диалог системным сообщением.
 */
function missingCapability(name: string): string {
  return (
    `У Yuki нет возможности «${name}». ` +
    'Скажи пользователю об этом и предложи добавить её через Capability Hub, ' +
    'спросив разрешение. Не придумывай, что действие выполнено.'
  )
}

function describeError(error: unknown): string {
  if (error instanceof Error) return error.message
  return String(error)
}

/** Короткое человекочитаемое описание результата для журнала и UI. */
function summarise(value: unknown): string {
  if (value === undefined || value === null) return 'готово'
  if (typeof value === 'string') return value
  const json = JSON.stringify(value)
  return json.length > 400 ? `${json.slice(0, 400)}…` : json
}

function toolResultBlock(id: string, content: string, isError: boolean): ContentBlock {
  return { type: 'tool_result', toolUseId: id, content, isError }
}

/**
 * Выполняет запрос пользователя целиком: от реплики до финального ответа.
 *
 * `history` не мутируется — возвращается новый массив, чтобы вызывающая сторона
 * сама решила, сохранять ли ход, если он оборвался ошибкой.
 */
export async function runAgent(
  history: readonly Message[],
  deps: AgentDeps,
): Promise<AgentOutcome> {
  const messages: Message[] = [...history]
  const maxSteps = deps.maxSteps ?? DEFAULT_MAX_STEPS
  const tools = deps.registry.specs()

  let reply = ''
  let steps = 0

  while (steps < maxSteps) {
    if (deps.signal?.aborted) {
      deps.onEvent({ kind: 'error', message: 'Задача отменена' })
      return { messages, reply, steps }
    }

    steps += 1

    const response = await deps.chat({
      // Снимок, а не живой массив: реализация `chat` уходит за границу процесса
      // и не должна видеть, как история меняется у неё под руками.
      messages: [...messages],
      tools,
      ...(deps.systemExtra ? { systemExtra: deps.systemExtra } : {}),
    })

    messages.push({ role: 'assistant', content: response.content })

    const text = responseText(response)
    if (text) {
      reply = text
      deps.onEvent({ kind: 'text', text })
    }

    const calls = toolUses(response)
    if (response.stopReason !== 'tool_use' || calls.length === 0) {
      deps.onEvent({ kind: 'done', steps })
      return { messages, reply, steps }
    }

    // OBSERVE: результаты всех вызовов одного хода возвращаются одним сообщением.
    // Разбивать их на несколько — значит ломать протокол и отучать модель
    // запрашивать инструменты параллельно.
    const results: ContentBlock[] = []

    for (const call of calls) {
      const tool = deps.registry.get(call.name)

      if (!tool) {
        // ТЗ §42: отсутствие возможности — это разговор, а не ошибка выполнения.
        deps.onEvent({ kind: 'blocked', toolId: call.name, reason: 'нет такой возможности' })
        deps.log?.({
          tool: call.name,
          status: 'denied',
          result: 'возможность не найдена',
          durationMs: 0,
        })
        results.push(toolResultBlock(call.id, missingCapability(call.name), true))
        continue
      }

      const decision = deps.decide(tool)

      if (decision.kind === 'deny') {
        const reason =
          decision.reason === 'os_permission_missing'
            ? `нет системного разрешения «${decision.category}»`
            : `пользователь не разрешил категорию «${decision.category}»`
        deps.onEvent({ kind: 'blocked', toolId: tool.id, reason })
        deps.log?.({ tool: tool.id, status: 'denied', result: reason, durationMs: 0 })
        results.push(
          toolResultBlock(
            call.id,
            `Действие не выполнено: ${reason}. Объясни это пользователю и предложи выдать разрешение в настройках.`,
            true,
          ),
        )
        continue
      }

      if (decision.kind === 'confirm') {
        const approval = await deps.confirm(tool, call.input)
        if (approval === 'deny') {
          deps.onEvent({ kind: 'blocked', toolId: tool.id, reason: 'пользователь отказал' })
          deps.log?.({ tool: tool.id, status: 'cancelled', durationMs: 0 })
          results.push(
            toolResultBlock(
              call.id,
              'Пользователь отменил действие. Не выполняй его и не считай выполненным.',
              true,
            ),
          )
          continue
        }
      }

      deps.onEvent({ kind: 'tool_started', toolId: tool.id, summary: tool.name })

      const started = performance.now()
      try {
        const value = await tool.execute(call.input, {
          signal: deps.signal ?? new AbortController().signal,
          report: (status) =>
            deps.onEvent({ kind: 'tool_started', toolId: tool.id, summary: status }),
        })
        const durationMs = Math.round(performance.now() - started)
        const detail = summarise(value)

        deps.onEvent({ kind: 'tool_finished', toolId: tool.id, ok: true, detail, durationMs })
        deps.log?.({ tool: tool.id, status: 'ok', result: detail, durationMs })
        results.push(toolResultBlock(call.id, detail, false))
      } catch (error) {
        const durationMs = Math.round(performance.now() - started)
        const message = describeError(error)

        deps.onEvent({
          kind: 'tool_finished',
          toolId: tool.id,
          ok: false,
          detail: message,
          durationMs,
        })
        deps.log?.({ tool: tool.id, status: 'error', result: message, durationMs })
        // Ошибка уходит модели как ошибка (ТЗ §33, шаг 5): она должна знать
        // реальную причину, чтобы выбрать fallback, а не повторять вслепую.
        results.push(toolResultBlock(call.id, message, true))
      }
    }

    messages.push({ role: 'user', content: results })
  }

  const message =
    'Задача оказалась длиннее, чем допускает один запрос. Скажи, продолжать ли.'
  deps.onEvent({ kind: 'error', message })
  return { messages, reply: reply || message, steps }
}

/** Ошибка инструмента в формате ядра — для реализаций, которым нужен явный тип. */
export function toolFailure(kind: ToolError['kind'], message: string): Error {
  const error = new Error(message)
  error.name = kind
  return error
}
