/**
 * Инструменты работы с плагинами (ТЗ §18, §20).
 *
 * Ими выполняется сценарий self-extension: готового решения нет, Yuki пишет
 * локальное расширение сама, а установка проходит через проверку и явное
 * согласие человека.
 *
 * Порядок здесь не косметический, а обязательный:
 * `scaffold_plugin` → правка файлов → `review_plugin` → рассказать человеку,
 * что плагин просит → `install_plugin`. Установка вслепую — это запуск чужого
 * кода с правами пользователя, и решение о ней принимает не модель.
 */

import type { Tool } from '@yuki/core'

import * as bridge from '../bridge'

const listPlugins: Tool = {
  id: 'list_plugins',
  name: 'Список плагинов',
  description:
    'Возвращает установленные плагины: идентификатор, версию, источник, папку, ' +
    'заявленные разрешения и инструменты. Нужен, чтобы не ставить второй такой же.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  execute: () => bridge.pluginList(),
}

const reviewPlugin: Tool = {
  id: 'review_plugin',
  name: 'Проверить плагин',
  description:
    'Читает манифест плагина в папке и возвращает то, что будет установлено: ' +
    'имя, версию, запрашиваемые разрешения, обещанные инструменты, точную ' +
    'команду запуска и список несоответствий манифеста. Ничего не запускает. ' +
    'Вызывай перед install_plugin и перескажи человеку результат своими словами: ' +
    'он должен понимать, что именно получит доступ к его машине.',
  permissions: ['files'],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: {
      path: { type: 'string', description: 'Папка с файлом yuki-plugin.json' },
    },
    required: ['path'],
    additionalProperties: false,
  },
  execute: (input) => bridge.pluginReview((input as { path: string }).path),
}

const scaffoldPlugin: Tool = {
  id: 'scaffold_plugin',
  name: 'Создать заготовку плагина',
  description:
    'Создаёт папку с рабочим MCP-сервером на Python и манифестом yuki-plugin.json, ' +
    'возвращает путь к ней. Заготовка уже отвечает на вызовы — в ней есть ' +
    'инструмент ping; свои инструменты добавляй правкой server.py: список TOOLS ' +
    'и ветки в call_tool, после чего обнови поле tools в манифесте. ' +
    'Установку не выполняет: для этого есть install_plugin.',
  permissions: ['files'],
  // Заготовка ничего не запускает и лежит в каталоге данных Yuki, но создаёт
  // файлы — человек должен видеть, что на диске появилось новое.
  risk: 'medium',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: {
      id: {
        type: 'string',
        description: 'Идентификатор: строчная латиница, цифры, дефис, подчёркивание',
      },
      name: { type: 'string' },
      description: { type: 'string' },
      permissions: {
        type: 'array',
        items: { type: 'string' },
        description:
          'Категории из ТЗ §21: microphone, screen_recording, accessibility, files, ' +
          'network, shell, camera, notifications, browser, external_services',
      },
    },
    required: ['id', 'name'],
    additionalProperties: false,
  },
  execute: async (input) => {
    const args = input as {
      id: string
      name: string
      description?: string
      permissions?: string[]
    }
    const path = await bridge.pluginScaffold(args)
    return {
      path,
      manifest: `${path}\\yuki-plugin.json`,
      server: `${path}\\server.py`,
    }
  },
}

const installPlugin: Tool = {
  id: 'install_plugin',
  name: 'Установить плагин',
  description:
    'Проверяет манифест, размещает плагин и запускает его сервер, после чего его ' +
    'инструменты становятся доступны. origin: local — скопировать папку, ' +
    'dev_folder — работать прямо из папки разработчика, git — склонировать ' +
    'репозиторий (нужен url), generated — созданное тобой через scaffold_plugin. ' +
    'Запускает чужой код с правами пользователя: сначала review_plugin, потом ' +
    'рассказать человеку, что плагин просит, и только потом установка.',
  permissions: ['files', 'shell'],
  // Установка запускает постороннюю программу. Это ровно тот случай, ради
  // которого в ТЗ §22 существует подтверждение с показом плана.
  risk: 'high',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: {
      origin: { type: 'string', enum: ['local', 'dev_folder', 'git', 'generated'] },
      path: { type: 'string', description: 'Папка плагина; для git не нужна' },
      url: { type: 'string', description: 'Адрес репозитория; только для git' },
    },
    required: ['origin'],
    additionalProperties: false,
  },
  execute: (input) =>
    bridge.pluginInstall(
      input as {
        origin: 'local' | 'dev_folder' | 'git' | 'generated'
        path?: string
        url?: string
      },
    ),
}

const removePlugin: Tool = {
  id: 'remove_plugin',
  name: 'Удалить плагин',
  description:
    'Отключает плагин, убирает его записи и удаляет файлы — кроме плагинов, ' +
    'подключённых из папки разработчика: их исходники остаются на месте.',
  permissions: ['files'],
  risk: 'high',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { id: { type: 'string' } },
    required: ['id'],
    additionalProperties: false,
  },
  execute: (input) => bridge.pluginRemove((input as { id: string }).id),
}

export const PLUGIN_TOOLS: readonly Tool[] = [
  listPlugins,
  reviewPlugin,
  scaffoldPlugin,
  installPlugin,
  removePlugin,
]
