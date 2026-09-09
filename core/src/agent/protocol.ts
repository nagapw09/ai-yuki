/**
 * Формат обмена с AI Provider Layer.
 *
 * Зеркалит типы Rust-крейта `yuki-ai`: имена полей и варианты совпадают один в
 * один, потому что значения ходят между процессами как JSON. Любое расхождение
 * здесь проявится не ошибкой типов, а неверным поведением в рантайме — поэтому
 * файл держится рядом с агентным циклом и правится вместе с Rust-стороной.
 */

export type Role = 'user' | 'assistant'

export type ContentBlock =
  | { readonly type: 'text'; readonly text: string }
  | {
      readonly type: 'tool_use'
      readonly id: string
      readonly name: string
      readonly input: unknown
    }
  | {
      readonly type: 'tool_result'
      readonly toolUseId: string
      readonly content: string
      readonly isError: boolean
    }
  /**
   * Блок рассуждений модели. Пользователю не показывается (ТЗ §15), но обязан
   * вернуться провайдеру без изменений на следующем шаге цикла.
   */
  | {
      readonly type: 'thinking'
      readonly text: string
      readonly signature: string | null
    }

export interface Message {
  readonly role: Role
  readonly content: readonly ContentBlock[]
}

/** Описание инструмента в том виде, в каком его получает модель. */
export interface ToolSpec {
  readonly name: string
  readonly description: string
  readonly inputSchema: Record<string, unknown>
}

export type StopReason = 'end_turn' | 'tool_use' | 'max_tokens' | 'other'

export interface Usage {
  readonly inputTokens: number
  readonly outputTokens: number
}

export interface ChatResponse {
  readonly content: readonly ContentBlock[]
  readonly stopReason: StopReason
  readonly usage: Usage
  readonly model: string
}

export interface ChatCall {
  readonly messages: readonly Message[]
  readonly tools: readonly ToolSpec[]
  /** Дополнение к системной инструкции: роль, память, контекст (ТЗ §9, §24). */
  readonly systemExtra?: string
}

/** Один обмен с моделью. Реализация живёт в приложении и ходит в Rust. */
export type ChatFn = (call: ChatCall) => Promise<ChatResponse>

/** Весь текст ответа. */
export function responseText(response: ChatResponse): string {
  return response.content
    .filter((b): b is Extract<ContentBlock, { type: 'text' }> => b.type === 'text')
    .map((b) => b.text)
    .join('')
}

/** Запрошенные вызовы инструментов. */
export function toolUses(
  response: ChatResponse,
): Extract<ContentBlock, { type: 'tool_use' }>[] {
  return response.content.filter(
    (b): b is Extract<ContentBlock, { type: 'tool_use' }> => b.type === 'tool_use',
  )
}
