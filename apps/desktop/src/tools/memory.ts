/**
 * Инструменты памяти и напоминаний (ТЗ §9, §25).
 *
 * Память Yuki не наполняется сама: модель решает, что запомнить, и делает это
 * явным вызовом. Так у пользователя остаётся ровно то, что он может увидеть
 * и удалить в разделе «Память» — требование ТЗ §9.
 */

import type { Tool } from '@yuki/core'

import * as bridge from '../bridge'

const remember: Tool = {
  id: 'remember',
  name: 'Запомнить',
  description:
    'Сохраняет факт в память. Типы: long_term — то, что верно всегда (имя, язык, ' +
    'предпочтения, любимые приложения); episodic — что произошло; session — контекст ' +
    'текущего разговора; short_term — рабочая заметка на время задачи. ' +
    'Указывай key для фактов, которые бывают в единственном экземпляре ' +
    '(«имя», «любимый браузер») — тогда новая запись заменит старую, а не добавится рядом. ' +
    'Не сохраняй пароли, ключи и то, что пользователь просил не запоминать.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: {
      kind: {
        type: 'string',
        enum: ['long_term', 'episodic', 'session', 'short_term'],
      },
      content: { type: 'string', description: 'Сам факт, одной фразой' },
      key: { type: 'string', description: 'Имя факта, если он единственный в своём роде' },
    },
    required: ['kind', 'content'],
    additionalProperties: false,
  },
  execute: (input) => {
    const args = input as { kind: bridge.MemoryKind; content: string; key?: string }
    return bridge.memorySave({
      kind: args.kind,
      content: args.content,
      ...(args.key ? { key: args.key } : {}),
      source: 'assistant',
    })
  },
}

const recall: Tool = {
  id: 'recall',
  name: 'Вспомнить',
  description:
    'Ищет в памяти по подстроке. Долгосрочные факты и так приходят в контексте ' +
    'каждого запроса — этот инструмент нужен, когда надо найти что-то конкретное ' +
    'из эпизодов или старых записей.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: {
      query: { type: 'string' },
      limit: { type: 'number', default: 10 },
    },
    required: ['query'],
    additionalProperties: false,
  },
  execute: (input) => {
    const { query, limit } = input as { query: string; limit?: number }
    return bridge.memorySearch(query, limit ?? 10)
  },
}

const forget: Tool = {
  id: 'forget',
  name: 'Забыть',
  description:
    'Удаляет запись памяти по id. Id берётся из результата recall. ' +
    'Вызывай, когда пользователь просит забыть или когда факт устарел и заменён.',
  permissions: [],
  // Удаление памяти необратимо и касается данных пользователя, но затрагивает
  // только то, что Yuki сама же и записала, — этого хватает на средний риск.
  risk: 'medium',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { id: { type: 'string' } },
    required: ['id'],
    additionalProperties: false,
  },
  execute: (input) => bridge.memoryDelete((input as { id: string }).id),
}

const createReminder: Tool = {
  id: 'create_reminder',
  name: 'Создать напоминание',
  description:
    'Ставит напоминание на конкретный момент. Время указывай в секундах Unix (UTC). ' +
    'Считай его от текущего времени, которое дано в контексте запроса, и учитывай, ' +
    'что пользователь называет время в своём часовом поясе. ' +
    'recurrence: daily — каждый день в это же время, weekly — раз в неделю.',
  permissions: ['notifications'],
  risk: 'low',
  idempotent: false,
  inputSchema: {
    type: 'object',
    properties: {
      text: { type: 'string', description: 'О чём напомнить' },
      dueAt: { type: 'number', description: 'Unix-время в секундах' },
      recurrence: { type: 'string', enum: ['daily', 'weekly'] },
    },
    required: ['text', 'dueAt'],
    additionalProperties: false,
  },
  execute: (input) => {
    const { text, dueAt, recurrence } = input as {
      text: string
      dueAt: number
      recurrence?: 'daily' | 'weekly'
    }
    return bridge.reminderCreate(text, Math.round(dueAt), recurrence)
  },
}

const listReminders: Tool = {
  id: 'list_reminders',
  name: 'Список напоминаний',
  description: 'Возвращает активные напоминания с их сроками.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  execute: () => bridge.reminderList(),
}

const sendNotification: Tool = {
  id: 'notify',
  name: 'Уведомление',
  description:
    'Показывает системное уведомление. Для того, что пользователь должен увидеть, ' +
    'даже если окно Yuki свёрнуто. Обычный ответ в чате уведомлением дублировать не надо.',
  permissions: ['notifications'],
  risk: 'low',
  idempotent: false,
  inputSchema: {
    type: 'object',
    properties: { title: { type: 'string' }, body: { type: 'string' } },
    required: ['title', 'body'],
    additionalProperties: false,
  },
  execute: (input) => {
    const { title, body } = input as { title: string; body: string }
    return bridge.notify(title, body)
  },
}

export const MEMORY_TOOLS: readonly Tool[] = [
  remember,
  recall,
  forget,
  createReminder,
  listReminders,
  sendNotification,
]
