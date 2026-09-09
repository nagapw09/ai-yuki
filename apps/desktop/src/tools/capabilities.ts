/**
 * Инструменты самораcширения (ТЗ §17, §18).
 *
 * Ими Yuki выполняет сценарий из ТЗ §17: проверяет, что уже умеет, ищет готовое
 * решение, предлагает подключить, просит ключ, устанавливает, проверяет и
 * сообщает результат. Без них «Юки, добавь возможность управлять Spotify»
 * упирается в человека, который должен сам пойти в настройки, — а ТЗ §42 требует
 * ровно обратного.
 */

import type { Tool } from '@yuki/core'

import * as bridge from '../bridge'

const listCapabilities: Tool = {
  id: 'list_capabilities',
  name: 'Что я умею',
  description:
    'Возвращает установленные возможности: их источник, состояние и инструменты. ' +
    'Первый шаг, когда пользователь просит о чём-то, чего ты, возможно, уже умеешь.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  execute: () => bridge.capabilityList(),
}

const findIntegration: Tool = {
  id: 'find_integration',
  name: 'Найти интеграцию',
  description:
    'Ищет готовую интеграцию в каталоге по свободному запросу («spotify», ' +
    '«база данных», «календарь»). Возвращает описание, нужен ли ключ и где его взять. ' +
    'Пустой результат означает, что готового решения нет — так и скажи пользователю, ' +
    'не выдумывай, что интеграция существует.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { query: { type: 'string' } },
    required: ['query'],
    additionalProperties: false,
  },
  execute: (input) => bridge.integrationsList((input as { query: string }).query),
}

const installIntegration: Tool = {
  id: 'install_integration',
  name: 'Установить интеграцию',
  description:
    'Ставит интеграцию из каталога и сразу проверяет её: подключается к серверу ' +
    'и забирает список инструментов. Если серверу нужен ключ, сперва спроси его ' +
    'у пользователя — без ключа установка вернёт ошибку. ' +
    'Успехом считается только реально поднявшийся сервер: если вернулась ошибка, ' +
    'возможность не установлена, и говорить обратное нельзя.',
  permissions: ['network', 'external_services'],
  // Установка запускает чужой код на машине пользователя — это всегда
  // осознанное решение человека, а не решение модели (ТЗ §22).
  risk: 'high',
  idempotent: false,
  inputSchema: {
    type: 'object',
    properties: {
      id: { type: 'string', description: 'Идентификатор из find_integration' },
      secret: { type: 'string', description: 'Ключ или токен, если он нужен' },
    },
    required: ['id'],
    additionalProperties: false,
  },
  execute: (input) => {
    const { id, secret } = input as { id: string; secret?: string }
    return bridge.integrationInstall(id, secret)
  },
}

const addMcpServer: Tool = {
  id: 'add_mcp_server',
  name: 'Добавить MCP-сервер',
  description:
    'Подключает произвольный MCP-сервер, которого нет в каталоге: локальный ' +
    'процесс (transport = stdio, command и args) или удалённый (transport = http, url). ' +
    'Все параметры бери у пользователя — не угадывай команду запуска и не подставляй ' +
    'пакеты, о которых он не говорил.',
  permissions: ['network', 'external_services'],
  risk: 'high',
  idempotent: false,
  inputSchema: {
    type: 'object',
    properties: {
      id: { type: 'string', description: 'Короткий идентификатор, латиницей' },
      label: { type: 'string', description: 'Название для интерфейса' },
      transport: { type: 'string', enum: ['stdio', 'http', 'sse'] },
      command: { type: 'string' },
      args: { type: 'array', items: { type: 'string' } },
      url: { type: 'string' },
      secret: { type: 'string' },
      secretEnv: {
        type: 'string',
        description: 'Имя переменной окружения, в которую подставить ключ',
      },
    },
    required: ['id', 'label', 'transport'],
    additionalProperties: false,
  },
  execute: (input) => {
    const args = input as Parameters<typeof bridge.mcpAdd>[0]
    return bridge.mcpAdd(args)
  },
}

const testCapability: Tool = {
  id: 'test_capability',
  name: 'Проверить возможность',
  description:
    'Переподключается к MCP-серверу и возвращает список его инструментов. ' +
    'Этим проверяют, что возможность действительно работает, и чинят её после сбоя.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { id: { type: 'string' } },
    required: ['id'],
    additionalProperties: false,
  },
  execute: (input) => bridge.mcpTest((input as { id: string }).id),
}

const removeCapability: Tool = {
  id: 'remove_capability',
  name: 'Удалить возможность',
  description:
    'Отключает и удаляет возможность вместе с её ключом. Вызывай только по прямой ' +
    'просьбе пользователя.',
  permissions: [],
  risk: 'high',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { id: { type: 'string' } },
    required: ['id'],
    additionalProperties: false,
  },
  execute: (input) => bridge.capabilityRemove((input as { id: string }).id),
}

export const CAPABILITY_TOOLS: readonly Tool[] = [
  listCapabilities,
  findIntegration,
  installIntegration,
  addMcpServer,
  testCapability,
  removeCapability,
]

/**
 * Превращает инструменты подключённых MCP-серверов в инструменты реестра.
 *
 * Разрешения и риск назначаются здесь, а не сервером: MCP их не описывает, а
 * пускать чужой инструмент мимо Permission Gate нельзя. Средний риск означает,
 * что по умолчанию Yuki спросит подтверждение — про чужой инструмент мы не
 * знаем, читает он или удаляет.
 */
export async function mcpTools(): Promise<Tool[]> {
  const specs = await bridge.mcpTools().catch(() => [])

  return specs.map((spec) => ({
    id: spec.id,
    name: `${spec.serverId}: ${spec.toolName}`,
    description: spec.description || `Инструмент ${spec.toolName} сервера ${spec.serverId}`,
    permissions: ['external_services'],
    risk: 'medium',
    idempotent: false,
    inputSchema: spec.inputSchema,
    execute: (input) => bridge.mcpCall(spec.serverId, spec.toolName, input ?? {}),
  }))
}
