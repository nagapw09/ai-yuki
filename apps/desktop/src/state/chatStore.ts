import type { Message } from '@yuki/core'
import { create } from 'zustand'

/** Состояние вызова инструмента в ленте чата (ТЗ §15). */
export interface ToolStatus {
  readonly id: string
  readonly toolId: string
  /** Безопасная формулировка: «Ищу файл…», «Открываю браузер…». */
  readonly label: string
  readonly state: 'running' | 'ok' | 'error' | 'blocked'
  readonly detail?: string
  readonly durationMs?: number
}

/** Реплика в ленте. */
export interface ChatEntry {
  readonly id: string
  readonly role: 'user' | 'assistant'
  readonly text: string
  readonly tools: readonly ToolStatus[]
}

/** Запрос подтверждения опасного действия (ТЗ §22). */
export interface PendingConfirmation {
  readonly toolId: string
  readonly toolName: string
  readonly risk: 'medium' | 'high'
  /** План действия: что именно будет сделано и с чем. */
  readonly plan: string
  readonly resolve: (approved: boolean) => void
}

interface ChatState {
  /** Лента для показа. */
  entries: ChatEntry[]
  /**
   * История в формате провайдера.
   *
   * Хранится отдельно от ленты намеренно: модели нужны блоки `tool_use` и
   * `tool_result`, которые пользователю показывать нельзя, а пользователю нужен
   * порядок и оформление, которые модели не нужны. Одна структура для двух задач
   * заставила бы прятать половину полей в каждом из мест.
   */
  history: Message[]
  /** Текст, который печатается прямо сейчас. */
  streaming: string
  running: boolean
  error: string | null
  confirmation: PendingConfirmation | null

  startTurn: (text: string) => void
  appendDelta: (delta: string) => void
  /** Закрывает ход ассистента: стрим превращается в реплику. */
  finishTurn: (text: string, history: Message[]) => void
  failTurn: (message: string) => void
  upsertTool: (status: ToolStatus) => void
  askConfirmation: (request: Omit<PendingConfirmation, 'resolve'>) => Promise<boolean>
  resolveConfirmation: (approved: boolean) => void
  clear: () => void
}

let counter = 0
const nextId = () => `${Date.now().toString(36)}-${(counter += 1).toString(36)}`

export const useChatStore = create<ChatState>((set, get) => ({
  entries: [],
  history: [],
  streaming: '',
  running: false,
  error: null,
  confirmation: null,

  startTurn: (text) =>
    set((s) => ({
      entries: [...s.entries, { id: nextId(), role: 'user', text, tools: [] }],
      streaming: '',
      running: true,
      error: null,
    })),

  appendDelta: (delta) => set((s) => ({ streaming: s.streaming + delta })),

  finishTurn: (text, history) =>
    set((s) => {
      // Статусы инструментов, накопленные за ход, переезжают в реплику ассистента.
      const pending = s.entries.at(-1)
      const tools = pending?.role === 'assistant' ? pending.tools : []
      const entries =
        pending?.role === 'assistant' ? s.entries.slice(0, -1) : [...s.entries]

      return {
        entries: [...entries, { id: nextId(), role: 'assistant', text, tools }],
        history,
        streaming: '',
        running: false,
      }
    }),

  failTurn: (message) => set({ running: false, streaming: '', error: message }),

  upsertTool: (status) =>
    set((s) => {
      // Статусы копятся в «черновой» реплике ассистента, которая создаётся
      // при первом же инструменте и закрывается вместе с ходом.
      const last = s.entries.at(-1)
      const draft: ChatEntry =
        last?.role === 'assistant'
          ? last
          : { id: nextId(), role: 'assistant', text: '', tools: [] }
      const rest = last?.role === 'assistant' ? s.entries.slice(0, -1) : s.entries

      const index = draft.tools.findIndex((t) => t.id === status.id)
      const tools =
        index === -1
          ? [...draft.tools, status]
          : draft.tools.map((t, i) => (i === index ? status : t))

      return { entries: [...rest, { ...draft, tools }] }
    }),

  askConfirmation: (request) =>
    new Promise<boolean>((resolve) => {
      set({ confirmation: { ...request, resolve } })
    }),

  resolveConfirmation: (approved) => {
    const pending = get().confirmation
    set({ confirmation: null })
    pending?.resolve(approved)
  },

  clear: () => set({ entries: [], history: [], streaming: '', error: null }),
}))
