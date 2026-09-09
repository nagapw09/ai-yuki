import { describe, expect, it } from 'vitest'

import type { Tool } from '../agent/types'
import { evaluate, permissionMap } from './gate'
import type { GateSettings } from './gate'

function tool(overrides: Partial<Tool> = {}): Tool {
  return {
    id: 'test.tool',
    name: 'Тестовый инструмент',
    description: '',
    permissions: ['files'],
    risk: 'low',
    idempotent: true,
    inputSchema: {},
    execute: async () => undefined,
    ...overrides,
  }
}

function settings(overrides: Partial<GateSettings> = {}): GateSettings {
  return {
    permissions: permissionMap({ files: { granted: true, osGranted: true } }),
    mediumRiskPolicy: 'auto',
    ...overrides,
  }
}

describe('permission gate', () => {
  it('пропускает низкий риск при выданных разрешениях', () => {
    expect(evaluate(tool(), settings())).toEqual({ kind: 'allow' })
  })

  it('всегда требует подтверждения для высокого риска', () => {
    expect(evaluate(tool({ risk: 'high' }), settings())).toEqual({
      kind: 'confirm',
      risk: 'high',
    })
  })

  it('для среднего риска следует настройке пользователя', () => {
    expect(evaluate(tool({ risk: 'medium' }), settings())).toEqual({ kind: 'allow' })
    expect(evaluate(tool({ risk: 'medium' }), settings({ mediumRiskPolicy: 'ask' }))).toEqual({
      kind: 'confirm',
      risk: 'medium',
    })
  })

  it('отказывает, когда разрешение ОС не выдано', () => {
    const decision = evaluate(
      tool({ risk: 'high' }),
      settings({ permissions: permissionMap({ files: { granted: true, osGranted: false } }) }),
    )
    expect(decision).toEqual({
      kind: 'deny',
      reason: 'os_permission_missing',
      category: 'files',
    })
  })

  it('отказывает, когда пользователь не разрешил категорию', () => {
    const decision = evaluate(
      tool(),
      settings({ permissions: permissionMap({ files: { granted: false, osGranted: true } }) }),
    )
    expect(decision).toEqual({
      kind: 'deny',
      reason: 'user_permission_missing',
      category: 'files',
    })
  })

  it('отсутствие категории в настройках трактуется как отказ, а не как разрешение', () => {
    const decision = evaluate(
      tool({ permissions: ['shell'] }),
      settings({ permissions: permissionMap({}) }),
    )
    expect(decision).toEqual({
      kind: 'deny',
      reason: 'os_permission_missing',
      category: 'shell',
    })
  })

  it('проверяет все категории инструмента, а не только первую', () => {
    const decision = evaluate(
      tool({ permissions: ['files', 'shell'] }),
      settings({
        permissions: permissionMap({
          files: { granted: true, osGranted: true },
          shell: { granted: false, osGranted: true },
        }),
      }),
    )
    expect(decision).toEqual({
      kind: 'deny',
      reason: 'user_permission_missing',
      category: 'shell',
    })
  })

  it('отказ в разрешении важнее подтверждения риска', () => {
    // Спрашивать подтверждение действия, которое всё равно будет отклонено ОС,
    // значит тревожить пользователя впустую.
    const decision = evaluate(
      tool({ risk: 'high' }),
      settings({ permissions: permissionMap({ files: { granted: false, osGranted: false } }) }),
    )
    expect(decision.kind).toBe('deny')
  })
})
