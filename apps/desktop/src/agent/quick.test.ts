import { describe, expect, it, vi } from 'vitest'

vi.mock('../bridge', () => ({}))
vi.mock('./session', () => ({ toolRegistry: () => ({ get: () => undefined }) }))
vi.mock('./commands', () => ({ gateSettings: async () => ({}) }))

import { recognize } from './quick'
import { moodStreamFilter, parseMood, speakable } from './reply'

describe('быстрые команды', () => {
  it('узнают частые просьбы в том виде, в каком их отдаёт распознавание', () => {
    expect(recognize('Открой браузер.')).toMatchObject({ kind: 'open', tool: 'open_url' })
    expect(recognize('запусти блокнот')).toMatchObject({ kind: 'open', target: 'notepad' })
    expect(recognize('Стоп!')).toEqual({ kind: 'stop' })
    expect(recognize('поставь на паузу')).toEqual({ kind: 'media', action: 'pause' })
    expect(recognize('Громкость на 40 процентов')).toEqual({ kind: 'volume', level: 0.4 })
    expect(recognize('Станцуй, пожалуйста')).toMatchObject({ kind: 'avatar', action: 'builtin:dance' })
    expect(recognize('Который час?')).toEqual({ kind: 'time' })
  })

  it('незнакомое оставляют модели', () => {
    expect(recognize('включи что-нибудь спокойное')).toBeNull()
    expect(recognize('открой последний документ')).toBeNull()
    expect(recognize('как дела')).toBeNull()
    expect(recognize('стоп, а что было вчера?')).toBeNull()
  })
})

describe('ответ модели', () => {
  it('метка настроения уходит на лицо, а не в текст', () => {
    expect(parseMood('[радость] Готово, открыла!')).toEqual({ text: 'Готово, открыла!', expression: 'happy' })
    expect(parseMood('Просто ответ')).toEqual({ text: 'Просто ответ', expression: null })
    expect(parseMood('[что-то] ответ').expression).toBeNull()
  })

  it('метка не мелькает в потоке, даже разрезанная на куски', () => {
    const filter = moodStreamFilter()
    const shown = ['[ра', 'дость', '] При', 'вет'].map(filter).join('')
    expect(shown).toBe('Привет')
    const plain = moodStreamFilter()
    expect(['При', 'вет'].map(plain).join('')).toBe('Привет')
  })

  it('вслух не читается разметка и ссылки', () => {
    expect(speakable('**Готово.** Вот [ссылка](https://x.ru) и `код`')).toBe('Готово. Вот ссылка и код')
    expect(speakable('- раз\n- два')).toBe('раз два')
    const long = 'Первое предложение довольно длинное. '.repeat(20)
    expect(speakable(long).length).toBeLessThanOrEqual(320)
    expect(speakable(long).endsWith('.')).toBe(true)
  })
})
