import { describe, expect, it, vi } from 'vitest'

import { permissionMap } from '../permissions/gate'
import type { GateDecision } from '../permissions/gate'
import { runAgent } from './loop'
import type { AgentDeps, AgentEvent } from './loop'
import type { ChatResponse, Message } from './protocol'
import { ToolRegistry } from './registry'
import type { Tool } from './types'

function textResponse(text: string): ChatResponse {
  return {
    content: [{ type: 'text', text }],
    stopReason: 'end_turn',
    usage: { inputTokens: 0, outputTokens: 0 },
    model: 'test',
  }
}

function toolResponse(name: string, input: unknown = {}): ChatResponse {
  return {
    content: [{ type: 'tool_use', id: 'call_1', name, input }],
    stopReason: 'tool_use',
    usage: { inputTokens: 0, outputTokens: 0 },
    model: 'test',
  }
}

function makeTool(overrides: Partial<Tool> = {}): Tool {
  return {
    id: 'open_app',
    name: 'Открыть приложение',
    description: 'Запускает приложение по имени',
    permissions: [],
    risk: 'low',
    idempotent: true,
    inputSchema: { type: 'object' },
    execute: async () => ({ pid: 42 }),
    ...overrides,
  }
}

/** Отдаёт заранее заготовленные ответы модели по одному на вызов. */
function scriptedChat(responses: ChatResponse[]) {
  let index = 0
  return vi.fn(async () => {
    const response = responses[index]
    index += 1
    if (!response) throw new Error('модель вызвана больше раз, чем задано в сценарии')
    return response
  })
}

function deps(overrides: Partial<AgentDeps> & Pick<AgentDeps, 'chat'>): {
  deps: AgentDeps
  events: AgentEvent[]
} {
  const events: AgentEvent[] = []
  return {
    events,
    deps: {
      registry: new ToolRegistry(),
      decide: (): GateDecision => ({ kind: 'allow' }),
      confirm: async () => 'allow',
      onEvent: (e) => events.push(e),
      ...overrides,
    },
  }
}

const ASK: readonly Message[] = [{ role: 'user', content: [{ type: 'text', text: 'привет' }] }]

describe('agent loop', () => {
  it('возвращает ответ без инструментов, когда модель закончила ход', async () => {
    const { deps: d } = deps({ chat: scriptedChat([textResponse('Привет!')]) })
    const outcome = await runAgent(ASK, d)

    expect(outcome.reply).toBe('Привет!')
    expect(outcome.steps).toBe(1)
  })

  it('выполняет инструмент и возвращает модели настоящий результат', async () => {
    const execute = vi.fn(async () => ({ pid: 42 }))
    const registry = new ToolRegistry().register(makeTool({ execute }))
    const chat = scriptedChat([toolResponse('open_app', { app: 'Chrome' }), textResponse('Готово')])

    const { deps: d } = deps({ chat, registry })
    const outcome = await runAgent(ASK, d)

    expect(execute).toHaveBeenCalledWith({ app: 'Chrome' }, expect.anything())
    expect(outcome.reply).toBe('Готово')

    // Модель должна была получить результат вторым запросом.
    const secondCall = chat.mock.calls[1]?.[0]
    const lastMessage = secondCall?.messages.at(-1)
    expect(lastMessage?.content[0]).toMatchObject({
      type: 'tool_result',
      toolUseId: 'call_1',
      isError: false,
    })
  })

  it('сообщает модели об ошибке инструмента, а не проглатывает её', async () => {
    const registry = new ToolRegistry().register(
      makeTool({
        execute: async () => {
          throw new Error('приложение не найдено')
        },
      }),
    )
    const chat = scriptedChat([toolResponse('open_app'), textResponse('Не получилось')])

    const { deps: d, events } = deps({ chat, registry })
    await runAgent(ASK, d)

    const result = chat.mock.calls[1]?.[0].messages.at(-1)?.content[0]
    expect(result).toMatchObject({ isError: true, content: 'приложение не найдено' })
    expect(events).toContainEqual(
      expect.objectContaining({ kind: 'tool_finished', ok: false }),
    )
  })

  it('не выдумывает возможность, которой нет (ТЗ §42)', async () => {
    const chat = scriptedChat([toolResponse('control_spotify'), textResponse('Пока не умею')])
    const { deps: d, events } = deps({ chat })

    await runAgent(ASK, d)

    const result = chat.mock.calls[1]?.[0].messages.at(-1)?.content[0]
    expect(result).toMatchObject({ isError: true })
    expect((result as { content: string }).content).toContain('Capability Hub')
    expect(events).toContainEqual(
      expect.objectContaining({ kind: 'blocked', toolId: 'control_spotify' }),
    )
  })

  it('не выполняет инструмент, если разрешение не выдано', async () => {
    const execute = vi.fn()
    const registry = new ToolRegistry().register(
      makeTool({ permissions: ['files'], execute }),
    )
    const chat = scriptedChat([toolResponse('open_app'), textResponse('Нужно разрешение')])

    const { deps: d } = deps({
      chat,
      registry,
      decide: () => ({
        kind: 'deny',
        reason: 'os_permission_missing',
        category: 'files',
      }),
    })

    await runAgent(ASK, d)
    expect(execute).not.toHaveBeenCalled()
  })

  it('спрашивает подтверждение для высокого риска и уважает отказ', async () => {
    const execute = vi.fn()
    const registry = new ToolRegistry().register(
      makeTool({ id: 'file_delete', risk: 'high', execute }),
    )
    const confirm = vi.fn(async () => 'deny' as const)
    const chat = scriptedChat([toolResponse('file_delete'), textResponse('Отменено')])

    const { deps: d } = deps({
      chat,
      registry,
      confirm,
      decide: () => ({ kind: 'confirm', risk: 'high' }),
    })

    await runAgent(ASK, d)

    expect(confirm).toHaveBeenCalledTimes(1)
    expect(execute).not.toHaveBeenCalled()

    const result = chat.mock.calls[1]?.[0].messages.at(-1)?.content[0]
    expect((result as { content: string }).content).toContain('отменил')
  })

  it('выполняет действие после подтверждения пользователя', async () => {
    const execute = vi.fn(async () => 'удалено')
    const registry = new ToolRegistry().register(
      makeTool({ id: 'file_delete', risk: 'high', execute }),
    )
    const chat = scriptedChat([toolResponse('file_delete'), textResponse('Удалила')])

    const { deps: d } = deps({
      chat,
      registry,
      confirm: async () => 'allow',
      decide: () => ({ kind: 'confirm', risk: 'high' }),
    })

    await runAgent(ASK, d)
    expect(execute).toHaveBeenCalledTimes(1)
  })

  it('останавливается на потолке шагов, а не крутится вечно', async () => {
    const registry = new ToolRegistry().register(
      makeTool({
        execute: async () => {
          throw new Error('всё время падает')
        },
      }),
    )
    // Модель упорно просит один и тот же инструмент.
    const chat = vi.fn(async () => toolResponse('open_app'))

    const { deps: d, events } = deps({ chat, registry, maxSteps: 3 })
    const outcome = await runAgent(ASK, d)

    expect(outcome.steps).toBe(3)
    expect(chat).toHaveBeenCalledTimes(3)
    expect(events.at(-1)).toMatchObject({ kind: 'error' })
  })

  it('пишет в журнал каждый вызов инструмента (ТЗ §23)', async () => {
    const registry = new ToolRegistry().register(makeTool())
    const log = vi.fn()
    const chat = scriptedChat([toolResponse('open_app'), textResponse('Готово')])

    const { deps: d } = deps({ chat, registry, log })
    await runAgent(ASK, d)

    expect(log).toHaveBeenCalledWith(
      expect.objectContaining({ tool: 'open_app', status: 'ok' }),
    )
  })

  it('не трогает переданную историю', async () => {
    const history: readonly Message[] = ASK
    const { deps: d } = deps({ chat: scriptedChat([textResponse('ок')]) })

    await runAgent(history, d)
    expect(history).toHaveLength(1)
  })
})

describe('tool registry', () => {
  it('отдаёт модели описания зарегистрированных инструментов', () => {
    const registry = new ToolRegistry().register(makeTool())
    expect(registry.specs()).toEqual([
      {
        name: 'open_app',
        description: 'Запускает приложение по имени',
        inputSchema: { type: 'object' },
      },
    ])
  })

  it('не позволяет молча подменить инструмент', () => {
    const registry = new ToolRegistry().register(makeTool())
    expect(() => registry.register(makeTool())).toThrow(/уже зарегистрирован/)
  })

  it('строит карту разрешений из объекта', () => {
    const map = permissionMap({ files: { granted: true, osGranted: true } })
    expect(map.get('files')).toEqual({ granted: true, osGranted: true })
  })
})
