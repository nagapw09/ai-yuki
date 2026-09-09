/**
 * Инструменты календаря (ТЗ §25).
 *
 * Календарь принадлежит человеку и виден другим людям: приглашённые увидят
 * созданное событие, а удалённое исчезнет у всех. Поэтому запись и удаление
 * здесь — действия, требующие подтверждения, а не «просто вызов API».
 *
 * Времена передаются в RFC 3339 с зоной. Текущий момент и зона приходят
 * в системном сообщении каждого запроса, так что «завтра в 10» модель считает
 * сама — но считает от известного ей момента, а не от догадки.
 */

import type { Tool } from '@yuki/core'

import * as bridge from '../bridge'

const listAccounts: Tool = {
  id: 'calendar_accounts',
  name: 'Календари',
  description:
    'Показывает, какие календари подключены (google, microsoft) и настроены ли ' +
    'они. Вызывай первым, если не знаешь, каким календарём пользуется человек.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  execute: () => bridge.calendarAccounts(),
}

const listEvents: Tool = {
  id: 'calendar_events',
  name: 'События календаря',
  description:
    'Возвращает события в промежутке. from и to — моменты в RFC 3339 с зоной, ' +
    'например 2026-09-10T00:00:00+03:00. Повторяющиеся события приходят ' +
    'отдельными вхождениями.',
  permissions: ['network', 'external_services'],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: {
      provider: { type: 'string', enum: ['google', 'microsoft'] },
      from: { type: 'string' },
      to: { type: 'string' },
    },
    required: ['provider', 'from', 'to'],
    additionalProperties: false,
  },
  execute: (input) => {
    const args = input as { provider: string; from: string; to: string }
    return bridge.calendarEvents(args.provider, args.from, args.to)
  },
}

const createEvent: Tool = {
  id: 'create_calendar_event',
  name: 'Создать событие',
  description:
    'Создаёт событие в календаре. start и end — RFC 3339 с зоной. Возвращает ' +
    'событие таким, каким его принял сервис: время могло быть приведено к зоне ' +
    'календаря, и человеку надо сообщить именно результат.',
  permissions: ['network', 'external_services'],
  // Событие увидят приглашённые и оно попадёт в чужие уведомления —
  // это действие вовне, а не запись в локальную базу.
  risk: 'medium',
  idempotent: false,
  inputSchema: {
    type: 'object',
    properties: {
      provider: { type: 'string', enum: ['google', 'microsoft'] },
      title: { type: 'string' },
      start: { type: 'string' },
      end: { type: 'string' },
      location: { type: 'string' },
      description: { type: 'string' },
    },
    required: ['provider', 'title', 'start', 'end'],
    additionalProperties: false,
  },
  execute: (input) => {
    const args = input as {
      provider: string
      title: string
      start: string
      end: string
      location?: string
      description?: string
    }
    return bridge.calendarCreateEvent(args.provider, args)
  },
}

const deleteEvent: Tool = {
  id: 'delete_calendar_event',
  name: 'Удалить событие',
  description:
    'Удаляет событие по идентификатору из calendar_events. Отменённая встреча ' +
    'исчезает и у приглашённых — сначала убедись, что речь именно об этом событии.',
  permissions: ['network', 'external_services'],
  risk: 'high',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: {
      provider: { type: 'string', enum: ['google', 'microsoft'] },
      id: { type: 'string' },
    },
    required: ['provider', 'id'],
    additionalProperties: false,
  },
  execute: (input) => {
    const args = input as { provider: string; id: string }
    return bridge.calendarDeleteEvent(args.provider, args.id)
  },
}

export const CALENDAR_TOOLS: readonly Tool[] = [
  listAccounts,
  listEvents,
  createEvent,
  deleteEvent,
]
