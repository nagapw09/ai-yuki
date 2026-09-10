/**
 * Повседневные инструменты: заметки, погода, курсы (`docs/GAPS.md` §5, §6).
 *
 * В ТЗ их нет, но именно это спрашивают у ассистента каждый день. Заметки
 * держатся отдельно от памяти (ТЗ §9) намеренно: память Yuki читает сама и
 * подмешивает в контекст каждого запроса, а заметка — текст, который человек
 * написал для себя. Смешать их значит либо утопить память в чужих черновиках,
 * либо молча отправлять личные записи в модель.
 */

import type { Tool } from '@yuki/core'

import * as bridge from '../bridge'

const saveNote: Tool = {
  id: 'save_note',
  name: 'Сохранить заметку',
  description:
    'Записывает текст в заметки. Заголовок можно не указывать — он возьмётся ' +
    'из первой строки. Чтобы изменить существующую заметку, передай её id ' +
    'из list_notes. Заметки Yuki не читает без просьбы: это не память.',
  permissions: [],
  risk: 'low',
  idempotent: false,
  inputSchema: {
    type: 'object',
    properties: {
      body: { type: 'string' },
      title: { type: 'string' },
      id: { type: 'string', description: 'Для правки существующей заметки' },
      pinned: { type: 'boolean', description: 'Закрепить наверху списка' },
    },
    required: ['body'],
    additionalProperties: false,
  },
  execute: (input) =>
    bridge.noteSave(
      input as { body: string; title?: string; id?: string; pinned?: boolean },
    ),
}

const listNotes: Tool = {
  id: 'list_notes',
  name: 'Найти заметки',
  description:
    'Возвращает заметки: закреплённые сверху, дальше свежие. С параметром ' +
    'query ищет по заголовку и тексту.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { query: { type: 'string' } },
    additionalProperties: false,
  },
  execute: (input) => bridge.noteList((input as { query?: string }).query),
}

const deleteNote: Tool = {
  id: 'delete_note',
  name: 'Удалить заметку',
  description: 'Удаляет заметку по идентификатору из list_notes.',
  permissions: [],
  // Заметку писал человек, и восстановить её неоткуда.
  risk: 'high',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { id: { type: 'string' } },
    required: ['id'],
    additionalProperties: false,
  },
  execute: (input) => bridge.noteDelete((input as { id: string }).id),
}

const weather: Tool = {
  id: 'weather',
  name: 'Погода',
  description:
    'Погода сейчас и прогноз на ближайшие дни для города. Город указывай так, ' +
    'как его назвал человек — «Питер», «Москва», «Berlin». Температура в ' +
    'градусах Цельсия, ветер в км/ч.',
  permissions: ['network', 'external_services'],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { city: { type: 'string' } },
    required: ['city'],
    additionalProperties: false,
  },
  execute: (input) => bridge.weatherGet((input as { city: string }).city),
}

const rates: Tool = {
  id: 'currency_rates',
  name: 'Курсы валют',
  description:
    'Курс базовой валюты к остальным по данным Европейского центробанка. ' +
    'base — трёхбуквенный код, по умолчанию USD; symbols — к каким считать. ' +
    'Курсы обновляются раз в сутки, в выходные стоят на пятничных значениях — ' +
    'дата ответа показывает, на какой день они актуальны.',
  permissions: ['network', 'external_services'],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: {
      base: { type: 'string' },
      symbols: { type: 'array', items: { type: 'string' } },
    },
    additionalProperties: false,
  },
  execute: (input) => {
    const args = input as { base?: string; symbols?: string[] }
    return bridge.ratesGet(args.base, args.symbols)
  },
}

export const EVERYDAY_TOOLS: readonly Tool[] = [
  saveNote,
  listNotes,
  deleteNote,
  weather,
  rates,
]
