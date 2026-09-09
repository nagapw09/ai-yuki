/**
 * Инструменты работы с командами (ТЗ §16).
 *
 * Ими выполняется сценарий из ТЗ: «Юки, создай команду „Работа“, которая
 * открывает Chrome, Slack, Notion и VS Code». Без них команду может завести
 * только человек руками, а ТЗ §16 прямо требует, чтобы Yuki создавала
 * автоматизацию сама.
 */

import type { Tool } from '@yuki/core'

import * as bridge from '../bridge'

/** Схема шага — её же читает модель, поэтому описания подробные. */
const STEP_SCHEMA = {
  type: 'object',
  properties: {
    kind: {
      type: 'string',
      enum: ['action', 'delay', 'if'],
      description: 'action — вызов инструмента, delay — пауза, if — ветвление',
    },
    toolId: { type: 'string', description: 'Для action: идентификатор инструмента' },
    input: { type: 'object', description: 'Для action: аргументы инструмента' },
    saveAs: {
      type: 'string',
      description: 'Для action: имя, под которым результат будет доступен как {{имя}}',
    },
    ms: { type: 'number', description: 'Для delay: длительность паузы' },
    condition: {
      type: 'object',
      description: 'Для if: {left, op, right}; op — contains, equals, empty и т.п.',
    },
    then: { type: 'array', description: 'Для if: шаги ветки «да»' },
    otherwise: { type: 'array', description: 'Для if: шаги ветки «нет»' },
  },
  required: ['kind'],
} as const

const createCommand: Tool = {
  id: 'create_command',
  name: 'Создать команду',
  description:
    'Записывает последовательность шагов как команду. Шаги выполняются потом ' +
    'без участия модели — мгновенно и без расхода токенов, поэтому в команду ' +
    'имеет смысл складывать то, что повторяется. ' +
    'trigger: phrase — запуск по фразе (укажи phrase), hotkey — по сочетанию ' +
    '(укажи hotkey вида Ctrl+Alt+W), startup — при запуске Yuki, manual — только вручную. ' +
    'В шагах используй идентификаторы инструментов, которые у тебя есть; ' +
    'результат шага можно сохранить через saveAs и подставить дальше как {{имя}}.',
  permissions: [],
  // Команда — это записанная последовательность действий, которая потом
  // выполняется по фразе. Её создание стоит подтвердить: пользователь должен
  // увидеть, что именно Yuki собирается запомнить.
  risk: 'medium',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: {
      name: { type: 'string' },
      description: { type: 'string' },
      trigger: { type: 'string', enum: ['phrase', 'hotkey', 'startup', 'manual'] },
      phrase: { type: 'string' },
      hotkey: { type: 'string' },
      steps: { type: 'array', items: STEP_SCHEMA },
    },
    required: ['name', 'trigger', 'steps'],
    additionalProperties: false,
  },
  execute: async (input) => {
    const args = input as {
      name: string
      description?: string
      trigger: 'phrase' | 'hotkey' | 'startup' | 'manual'
      phrase?: string
      hotkey?: string
      steps: unknown[]
    }

    // Идентификатор строим из времени, а не из названия: два «Рабочих режима»
    // с разными шагами должны сосуществовать, а не затирать друг друга.
    const id = `cmd-${Date.now().toString(36)}`

    await bridge.commandSave({
      id,
      name: args.name,
      description: args.description ?? '',
      triggerKind: args.trigger,
      phrase: args.phrase ?? null,
      hotkey: args.hotkey ?? null,
      enabled: true,
      steps: args.steps,
    })

    return { id, name: args.name, steps: args.steps.length }
  },
}

const listCommands: Tool = {
  id: 'list_commands',
  name: 'Список команд',
  description:
    'Возвращает сохранённые команды с их триггерами и шагами. Нужен, чтобы ' +
    'не создавать вторую такую же и чтобы изменить существующую.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  execute: () => bridge.commandList(),
}

const deleteCommand: Tool = {
  id: 'delete_command',
  name: 'Удалить команду',
  description: 'Удаляет команду по идентификатору из list_commands.',
  permissions: [],
  risk: 'high',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { id: { type: 'string' } },
    required: ['id'],
    additionalProperties: false,
  },
  execute: (input) => bridge.commandDelete((input as { id: string }).id),
}

export const COMMAND_TOOLS: readonly Tool[] = [createCommand, listCommands, deleteCommand]
