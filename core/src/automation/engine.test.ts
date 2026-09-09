import { describe, expect, it, vi } from 'vitest'

import { ToolRegistry } from '../agent/registry'
import type { Tool } from '../agent/types'
import type { GateDecision } from '../permissions/gate'
import { evaluateCondition, interpolate, matchCommand, runCommand } from './engine'
import type { RunDeps } from './engine'
import type { Command, Step } from './types'

function tool(overrides: Partial<Tool> = {}): Tool {
  return {
    id: 'open_app',
    name: 'Открыть приложение',
    description: '',
    permissions: [],
    risk: 'low',
    idempotent: true,
    inputSchema: {},
    execute: async () => 'готово',
    ...overrides,
  }
}

function command(steps: readonly Step[], overrides: Partial<Command> = {}): Command {
  return {
    id: 'work',
    name: 'Рабочий режим',
    description: '',
    trigger: { kind: 'manual' },
    enabled: true,
    steps,
    ...overrides,
  }
}

function deps(registry: ToolRegistry, overrides: Partial<RunDeps> = {}): RunDeps {
  return {
    registry,
    decide: (): GateDecision => ({ kind: 'allow' }),
    confirm: async () => true,
    // Паузы в тестах не ждём: проверяется порядок, а не часы.
    sleep: async () => undefined,
    ...overrides,
  }
}

describe('подстановка переменных', () => {
  it('подставляет сохранённые значения', () => {
    expect(interpolate('открой {{файл}}', { файл: 'отчёт.pdf' })).toBe('открой отчёт.pdf')
  })

  it('оставляет неизвестное имя на месте', () => {
    // Молча подставленная пустота превратила бы команду в бессмыслицу,
    // которую уже не отладить.
    expect(interpolate('открой {{нет}}', {})).toBe('открой {{нет}}')
  })

  it('терпит пробелы внутри скобок', () => {
    expect(interpolate('{{ имя }}', { имя: 'Алекс' })).toBe('Алекс')
  })
})

describe('условия', () => {
  const vars = { текст: 'Привет, мир', пусто: '   ' }

  it('сравнивает по вхождению и на равенство', () => {
    expect(evaluateCondition({ left: '{{текст}}', op: 'contains', right: 'мир' }, vars)).toBe(true)
    expect(
      evaluateCondition({ left: '{{текст}}', op: 'not_contains', right: 'пока' }, vars),
    ).toBe(true)
    expect(
      evaluateCondition({ left: '{{текст}}', op: 'equals', right: 'Привет, мир' }, vars),
    ).toBe(true)
  })

  it('считает строку из пробелов пустой', () => {
    expect(evaluateCondition({ left: '{{пусто}}', op: 'empty' }, vars)).toBe(true)
    expect(evaluateCondition({ left: '{{текст}}', op: 'not_empty' }, vars)).toBe(true)
  })
})

describe('выполнение команды', () => {
  it('выполняет шаги по порядку', async () => {
    const order: string[] = []
    const registry = new ToolRegistry()
      .register(tool({ id: 'a', execute: async () => void order.push('a') }))
      .register(tool({ id: 'b', execute: async () => void order.push('b') }))

    const outcome = await runCommand(
      command([
        { kind: 'action', toolId: 'a', input: {} },
        { kind: 'action', toolId: 'b', input: {} },
      ]),
      deps(registry),
    )

    expect(outcome.ok).toBe(true)
    expect(order).toEqual(['a', 'b'])
    expect(outcome.executed).toBe(2)
  })

  it('передаёт результат шага в следующий', async () => {
    const execute = vi.fn(async () => undefined)
    const registry = new ToolRegistry()
      .register(tool({ id: 'read', execute: async () => 'отчёт.pdf' }))
      .register(tool({ id: 'open', execute }))

    await runCommand(
      command([
        { kind: 'action', toolId: 'read', input: {}, saveAs: 'файл' },
        { kind: 'action', toolId: 'open', input: { path: 'C:/{{файл}}' } },
      ]),
      deps(registry),
    )

    expect(execute).toHaveBeenCalledWith({ path: 'C:/отчёт.pdf' }, expect.anything())
  })

  it('идёт по ветке then и пропускает else', async () => {
    const taken = vi.fn(async () => undefined)
    const skipped = vi.fn(async () => undefined)
    const registry = new ToolRegistry()
      .register(tool({ id: 'check', execute: async () => 'всё хорошо' }))
      .register(tool({ id: 'yes', execute: taken }))
      .register(tool({ id: 'no', execute: skipped }))

    await runCommand(
      command([
        { kind: 'action', toolId: 'check', input: {}, saveAs: 'r' },
        {
          kind: 'if',
          condition: { left: '{{r}}', op: 'contains', right: 'хорошо' },
          then: [{ kind: 'action', toolId: 'yes', input: {} }],
          otherwise: [{ kind: 'action', toolId: 'no', input: {} }],
        },
      ]),
      deps(registry),
    )

    expect(taken).toHaveBeenCalled()
    expect(skipped).not.toHaveBeenCalled()
  })

  it('останавливается на первой ошибке, а не доламывает дальше', async () => {
    const after = vi.fn(async () => undefined)
    const registry = new ToolRegistry()
      .register(
        tool({
          id: 'bad',
          execute: async () => {
            throw new Error('файл не найден')
          },
        }),
      )
      .register(tool({ id: 'after', execute: after }))

    const outcome = await runCommand(
      command([
        { kind: 'action', toolId: 'bad', input: {} },
        { kind: 'action', toolId: 'after', input: {} },
      ]),
      deps(registry),
    )

    expect(outcome.ok).toBe(false)
    expect(outcome.error).toContain('файл не найден')
    expect(after).not.toHaveBeenCalled()
  })

  it('уважает Permission Gate так же, как действия модели', async () => {
    const execute = vi.fn()
    const registry = new ToolRegistry().register(tool({ execute }))

    const outcome = await runCommand(
      command([{ kind: 'action', toolId: 'open_app', input: {} }]),
      deps(registry, {
        decide: () => ({ kind: 'deny', reason: 'user_permission_missing', category: 'files' }),
      }),
    )

    expect(execute).not.toHaveBeenCalled()
    expect(outcome.ok).toBe(false)
    expect(outcome.error).toContain('files')
  })

  it('спрашивает подтверждение опасного шага и уважает отказ', async () => {
    const execute = vi.fn()
    const registry = new ToolRegistry().register(
      tool({ id: 'file_delete', risk: 'high', execute }),
    )

    const outcome = await runCommand(
      command([{ kind: 'action', toolId: 'file_delete', input: {} }]),
      deps(registry, {
        decide: () => ({ kind: 'confirm', risk: 'high' }),
        confirm: async () => false,
      }),
    )

    expect(execute).not.toHaveBeenCalled()
    expect(outcome.error).toContain('отменён')
  })

  it('останавливается, когда инструмент исчез вместе с возможностью', async () => {
    const outcome = await runCommand(
      command([{ kind: 'action', toolId: 'mcp__gone__do', input: {} }]),
      deps(new ToolRegistry()),
    )

    expect(outcome.ok).toBe(false)
    expect(outcome.error).toContain('больше нет')
  })

  it('не крутится бесконечно на вложенных ветках', async () => {
    const registry = new ToolRegistry().register(tool({ id: 'noop' }))

    // Двести шагов подряд — потолок должен сработать.
    const many: Step[] = Array.from({ length: 300 }, () => ({
      kind: 'action' as const,
      toolId: 'noop',
      input: {},
    }))

    const outcome = await runCommand(command(many), deps(registry))
    expect(outcome.ok).toBe(false)
    expect(outcome.error).toContain('зацикливание')
  })
})

describe('подбор команды по фразе', () => {
  const commands: Command[] = [
    command([], { id: 'a', trigger: { kind: 'phrase', phrase: 'режим' } }),
    command([], { id: 'b', trigger: { kind: 'phrase', phrase: 'рабочий режим' } }),
    command([], { id: 'c', trigger: { kind: 'phrase', phrase: 'выключено' }, enabled: false }),
  ]

  it('берёт самую конкретную из подошедших фраз', () => {
    expect(matchCommand('Юки, запусти рабочий режим', commands)?.id).toBe('b')
  })

  it('срабатывает на вхождение, а не на дословное совпадение', () => {
    expect(matchCommand('включи режим пожалуйста', commands)?.id).toBe('a')
  })

  it('не трогает выключенные команды', () => {
    expect(matchCommand('выключено', commands)).toBeUndefined()
  })

  it('ничего не находит в пустой реплике', () => {
    expect(matchCommand('   ', commands)).toBeUndefined()
  })
})
