import { describe, expect, it } from 'vitest'

import { builtinToolIds } from '../tools'
import { COMMAND_TEMPLATES, templateToolIds } from './templates'

/**
 * Библиотека готовых команд обязана ссылаться на настоящие инструменты
 * (`docs/GAPS.md` §9).
 *
 * Проверка не формальная: шаблон живёт в коде, инструменты — в другом файле, и
 * переименованный инструмент ломает шаблон молча. Обнаружилось бы это у
 * пользователя, который добавил команду из библиотеки и получил «инструмент
 * недоступен» вместо обещанного.
 */
describe('библиотека готовых команд', () => {
  const known = builtinToolIds()

  it('каждый шаг зовёт существующий инструмент', () => {
    for (const template of COMMAND_TEMPLATES) {
      for (const toolId of templateToolIds(template)) {
        expect(known.has(toolId), `«${template.name}» зовёт неизвестный ${toolId}`).toBe(true)
      }
    }
  })

  it('у каждого шаблона есть шаги и понятное описание', () => {
    for (const template of COMMAND_TEMPLATES) {
      expect(template.steps.length, template.name).toBeGreaterThan(0)
      // Описание читает человек, выбирающий из списка: пустое делает
      // библиотеку набором загадочных названий.
      expect(template.description.length, template.name).toBeGreaterThan(20)
    }
  })

  it('шаблон с запуском по фразе эту фразу задаёт', () => {
    for (const template of COMMAND_TEMPLATES) {
      if (template.triggerKind === 'phrase') {
        expect(template.phrase?.trim(), template.name).toBeTruthy()
      }
    }
  })

  it('фразы запуска не повторяются', () => {
    // Две команды на одну фразу — это команда, которая иногда запускает не то.
    const phrases = COMMAND_TEMPLATES.map((t) => t.phrase).filter(Boolean)
    expect(new Set(phrases).size).toBe(phrases.length)
  })

  it('идентификаторы шаблонов уникальны', () => {
    const ids = COMMAND_TEMPLATES.map((t) => t.id)
    expect(new Set(ids).size).toBe(ids.length)
  })

  it('собирает инструменты и из веток условия', () => {
    // Шаг внутри if — такой же шаг: пропустить его при проверке значит
    // проверять половину шаблона.
    const clipboard = COMMAND_TEMPLATES.find((t) => t.id === 'note-from-clipboard')
    expect(clipboard).toBeDefined()

    const ids = templateToolIds(clipboard!)
    expect(ids).toContain('clipboard_read')
    expect(ids).toContain('save_note')
    expect(ids).toContain('notify')
  })
})
