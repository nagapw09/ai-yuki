/**
 * Подключение агентного цикла к приложению (ТЗ §5).
 *
 * Здесь сходятся четыре вещи, которые ядро намеренно не знает: как позвать модель
 * (через Rust), какие разрешения выданы (из базы), как спросить подтверждение
 * (модалка) и куда писать журнал (ТЗ §23).
 */

import {
  ToolRegistry,
  evaluate,
  permissionMap,
  runAgent,
  type ChatFn,
  type ChatResponse,
  type GateSettings,
  type Message,
  type PermissionCategory,
  type PermissionState,
  type Tool,
} from '@yuki/core'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

import { activityRecord, permissionsList, settingGet } from '../bridge'
import { useChatStore } from '../state/chatStore'
import { useUiStore } from '../state/store'
import { BUILTIN_TOOLS } from '../tools/builtin'

/** Реестр создаётся один раз: инструменты не меняются в течение сессии. */
const registry = new ToolRegistry().registerAll(BUILTIN_TOOLS)

export function toolRegistry(): ToolRegistry {
  return registry
}

interface ChatDeltaEvent {
  requestId: string
  text: string
}

let requestCounter = 0

/**
 * Обмен с моделью через Rust с потоковой отдачей текста в UI.
 *
 * Подписка на события живёт ровно один запрос: слушатель, переживший свой вызов,
 * начал бы дописывать текст в следующий ход.
 */
function createChat(systemExtra: string | undefined): ChatFn {
  return async ({ messages, tools }) => {
    const requestId = `req-${(requestCounter += 1)}`

    const unlisten = await listen<ChatDeltaEvent>('yuki://chat-delta', (event) => {
      if (event.payload.requestId === requestId) {
        useChatStore.getState().appendDelta(event.payload.text)
      }
    })

    try {
      return await invoke<ChatResponse>('chat_send', {
        args: {
          requestId,
          messages,
          tools,
          systemExtra: systemExtra ?? null,
          providerId: null,
          model: null,
          maxTokens: null,
          temperature: null,
        },
      })
    } finally {
      unlisten()
    }
  }
}

/** Текущие разрешения из базы (ТЗ §21). */
async function loadGateSettings(): Promise<GateSettings> {
  const rows = await permissionsList()
  const entries: Partial<Record<PermissionCategory, PermissionState>> = {}
  for (const row of rows) {
    entries[row.category as PermissionCategory] = {
      granted: row.granted,
      osGranted: row.osGranted,
    }
  }

  const policy = await settingGet('confirm.medium_risk')
  return {
    permissions: permissionMap(entries),
    // По умолчанию средний риск спрашивает: молчаливое выполнение действия,
    // которое ТЗ §22 отнесло к спорным, — не то, чем стоит удивлять пользователя.
    mediumRiskPolicy: policy === 'auto' ? 'auto' : 'ask',
  }
}

/** Человеческое описание того, что именно произойдёт (ТЗ §22). */
function describePlan(tool: Tool, input: unknown): string {
  if (input && typeof input === 'object' && Object.keys(input).length > 0) {
    const args = Object.entries(input as Record<string, unknown>)
      .map(([key, value]) => `${key}: ${JSON.stringify(value)}`)
      .join('\n')
    return `${tool.name}\n\n${args}`
  }
  return tool.name
}

/**
 * Обрабатывает одну реплику пользователя целиком.
 *
 * Возвращается после того, как ход завершён: и успех, и ошибка уже отражены
 * в состоянии — вызывающему коду ничего доделывать не нужно.
 */
export async function sendMessage(text: string): Promise<void> {
  const chat = useChatStore.getState()
  const ui = useUiStore.getState()

  chat.startTurn(text)
  ui.setOrbState('thinking')
  ui.setHeadline(null)

  const userMessage: Message = { role: 'user', content: [{ type: 'text', text }] }
  const history = [...chat.history, userMessage]

  let settings: GateSettings
  try {
    settings = await loadGateSettings()
  } catch (error) {
    chat.failTurn(describeError(error))
    ui.flashResult('error')
    return
  }

  const persona = (await settingGet('persona.extra').catch(() => null)) ?? undefined

  try {
    const outcome = await runAgent(history, {
      chat: createChat(persona),
      registry,
      decide: (tool) => evaluate(tool, settings),

      confirm: async (tool, input) => {
        // Пока пользователь думает, Orb не должен изображать работу.
        useUiStore.getState().setOrbState('idle')
        const approved = await useChatStore.getState().askConfirmation({
          toolId: tool.id,
          toolName: tool.name,
          risk: tool.risk === 'high' ? 'high' : 'medium',
          plan: describePlan(tool, input),
        })
        useUiStore.getState().setOrbState('working')
        return approved ? 'allow' : 'deny'
      },

      onEvent: (event) => {
        const store = useChatStore.getState()
        switch (event.kind) {
          case 'tool_started':
            useUiStore.getState().setOrbState('working')
            store.upsertTool({
              id: event.toolId,
              toolId: event.toolId,
              label: event.summary,
              state: 'running',
            })
            break
          case 'tool_finished':
            store.upsertTool({
              id: event.toolId,
              toolId: event.toolId,
              label: registry.get(event.toolId)?.name ?? event.toolId,
              state: event.ok ? 'ok' : 'error',
              detail: event.detail,
              durationMs: event.durationMs,
            })
            useUiStore.getState().setOrbState('thinking')
            break
          case 'blocked':
            store.upsertTool({
              id: event.toolId,
              toolId: event.toolId,
              label: registry.get(event.toolId)?.name ?? event.toolId,
              state: 'blocked',
              detail: event.reason,
            })
            break
          default:
            break
        }
      },

      log: (entry) => {
        // Журнал не должен ронять ход: ТЗ §23 требует записи, но потеря строки
        // журнала — меньшая беда, чем потеря результата уже выполненной работы.
        void activityRecord(entry).catch(() => undefined)
      },
    })

    chat.finishTurn(outcome.reply, outcome.messages)
    ui.flashResult('success')
  } catch (error) {
    const message = describeError(error)
    useChatStore.getState().failTurn(message)
    useUiStore.getState().flashResult('error')
    void activityRecord({ tool: 'agent', status: 'error', result: message }).catch(
      () => undefined,
    )
  }
}

function describeError(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return 'Не удалось выполнить запрос'
}
