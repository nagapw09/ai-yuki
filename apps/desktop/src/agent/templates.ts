/**
 * Библиотека готовых команд (`docs/GAPS.md` §9).
 *
 * # Почему это заменяет маркетплейс, а не дополняет его
 *
 * ТЗ §17 сознательно отказывается от публичного магазина, и это правильно. Но
 * у магазина была вторая функция, которую отказ забрал вместе с первой:
 * стартовый набор рецептов, по которому человек понимает, **что вообще бывает**.
 * Пустой список команд ничего не подсказывает, а «создайте автоматизацию»
 * звучит как задача, а не как приглашение.
 *
 * # Правило, которому подчинены все шаблоны
 *
 * Шаблон — это заготовка, а не подписка. После добавления он становится
 * обычной командой пользователя: её можно править, переименовывать и удалять,
 * и она ничем не связана с этим файлом. Ничего не скачивается, ничего не
 * обновляется извне.
 *
 * Каждый шаг ссылается только на настоящий инструмент из реестра — за этим
 * следит тест: шаблон, зовущий несуществующий инструмент, сломался бы уже
 * у пользователя.
 */

import type { Step } from '@yuki/core'

export interface CommandTemplate {
  readonly id: string
  readonly name: string
  /** Зачем это нужно — человеку, который выбирает из списка. */
  readonly description: string
  readonly triggerKind: 'phrase' | 'hotkey' | 'startup' | 'manual'
  readonly phrase?: string
  /** Что стоит поправить после добавления. Пусто — можно пользоваться сразу. */
  readonly adjust?: string
  readonly steps: readonly Step[]
}

/** Двадцать пять минут работы — классический помидор. */
const POMODORO_MS = 25 * 60 * 1000

export const COMMAND_TEMPLATES: readonly CommandTemplate[] = [
  {
    id: 'work-mode',
    name: 'Рабочий режим',
    description: 'Открывает браузер и редактор, приглушает звук и сообщает, что всё готово.',
    triggerKind: 'phrase',
    phrase: 'рабочий режим',
    adjust: 'Замените названия приложений на свои.',
    steps: [
      { kind: 'action', toolId: 'open_app', input: { name: 'Chrome' } },
      { kind: 'action', toolId: 'open_app', input: { name: 'Code' } },
      // Пауза перед звуком: приложения на старте любят поздороваться звуком.
      { kind: 'delay', ms: 1500 },
      { kind: 'action', toolId: 'set_volume', input: { level: 0.3 } },
      {
        kind: 'action',
        toolId: 'notify',
        input: { title: 'Рабочий режим', body: 'Всё открыто, звук приглушён.' },
      },
    ],
  },
  {
    id: 'note-from-clipboard',
    name: 'Заметка из буфера',
    description: 'Кладёт то, что скопировано, в заметки — не переключаясь на Yuki.',
    triggerKind: 'phrase',
    phrase: 'запиши из буфера',
    steps: [
      // Результат шага сохраняется под именем и подставляется дальше как {{clip}}.
      { kind: 'action', toolId: 'clipboard_read', input: {}, saveAs: 'clip' },
      {
        kind: 'if',
        condition: { left: '{{clip}}', op: 'not_empty' },
        then: [
          { kind: 'action', toolId: 'save_note', input: { body: '{{clip}}' } },
          {
            kind: 'action',
            toolId: 'notify',
            input: { title: 'Записано', body: 'Буфер сохранён в заметки.' },
          },
        ],
        // Молча ничего не сделать — худший вариант: человек решит, что записалось.
        otherwise: [
          {
            kind: 'action',
            toolId: 'notify',
            input: { title: 'Буфер пуст', body: 'Записывать было нечего.' },
          },
        ],
      },
    ],
  },
  {
    id: 'pomodoro',
    name: 'Помодоро',
    description: 'Двадцать пять минут работы, потом напоминание о перерыве.',
    triggerKind: 'phrase',
    phrase: 'помодоро',
    steps: [
      {
        kind: 'action',
        toolId: 'notify',
        input: { title: 'Помодоро', body: 'Двадцать пять минут пошли.' },
      },
      { kind: 'delay', ms: POMODORO_MS },
      {
        kind: 'action',
        toolId: 'notify',
        input: { title: 'Перерыв', body: 'Двадцать пять минут прошли — встаньте.' },
      },
    ],
  },
  {
    id: 'quiet-mode',
    name: 'Тихий режим',
    description: 'Выключает звук одним словом — когда звонок начинается внезапно.',
    triggerKind: 'phrase',
    phrase: 'тихий режим',
    steps: [
      { kind: 'action', toolId: 'set_volume', input: { level: 0 } },
      {
        kind: 'action',
        toolId: 'notify',
        input: { title: 'Тихий режим', body: 'Звук выключен.' },
      },
    ],
  },
  {
    id: 'work-links',
    name: 'Рабочие ссылки',
    description: 'Открывает набор ссылок, с которых начинается день.',
    triggerKind: 'phrase',
    phrase: 'открой рабочие ссылки',
    adjust: 'Впишите свои адреса вместо примеров.',
    steps: [
      { kind: 'action', toolId: 'open_url', input: { url: 'https://mail.google.com' } },
      { kind: 'action', toolId: 'open_url', input: { url: 'https://github.com' } },
    ],
  },
  {
    id: 'end-of-day',
    name: 'Конец дня',
    description: 'Закрывает браузер, возвращает звук и подводит черту.',
    triggerKind: 'phrase',
    phrase: 'конец дня',
    adjust: 'Добавьте приложения, которые тоже стоит закрыть.',
    steps: [
      { kind: 'action', toolId: 'close_app', input: { name: 'Chrome' } },
      { kind: 'action', toolId: 'set_volume', input: { level: 0.5 } },
      {
        kind: 'action',
        toolId: 'notify',
        input: { title: 'День закрыт', body: 'До завтра.' },
      },
    ],
  },
]

/** Идентификаторы инструментов, которые использует шаблон, включая ветки. */
export function templateToolIds(template: CommandTemplate): string[] {
  const collect = (steps: readonly Step[]): string[] =>
    steps.flatMap((step) => {
      if (step.kind === 'action') return [step.toolId]
      if (step.kind === 'if') return [...collect(step.then), ...collect(step.otherwise ?? [])]
      return []
    })

  return collect(template.steps)
}
