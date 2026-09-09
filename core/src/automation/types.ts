/**
 * Модель автоматизаций (ТЗ §16).
 *
 * Команда — это последовательность шагов, а не диалог с моделью. В этом её
 * смысл: «Юки, запусти рабочий режим» должно открыть четыре приложения сразу,
 * а не рассуждать о том, какие именно приложения имелись в виду. Модель нужна,
 * чтобы команду **создать**; выполняется она без неё — быстро, предсказуемо
 * и без расхода токенов.
 */

/** Что запускает команду (ТЗ §16: triggers). */
export type Trigger =
  /** Фраза пользователя: «запусти рабочий режим». */
  | { readonly kind: 'phrase'; readonly phrase: string }
  /** Глобальное сочетание клавиш. */
  | { readonly kind: 'hotkey'; readonly shortcut: string }
  /** При запуске приложения. */
  | { readonly kind: 'startup' }
  /** Только вручную, из списка команд. */
  | { readonly kind: 'manual' }

/** Шаг программы (ТЗ §16: actions и logic). */
export type Step =
  /**
   * Вызов инструмента — тот же, что доступен модели.
   *
   * Отдельного языка действий нет намеренно: у Yuki уже есть реестр
   * инструментов с разрешениями и уровнями риска, и второй набор действий рядом
   * с ним означал бы вторую точку, где нужно проверять права.
   */
  | {
      readonly kind: 'action'
      readonly toolId: string
      readonly input: Record<string, unknown>
      /** Имя, под которым результат станет доступен следующим шагам. */
      readonly saveAs?: string
    }
  /** Пауза. */
  | { readonly kind: 'delay'; readonly ms: number }
  /** Ветвление. */
  | {
      readonly kind: 'if'
      readonly condition: Condition
      readonly then: readonly Step[]
      readonly otherwise?: readonly Step[]
    }

export interface Condition {
  /** Левая часть; поддерживает подстановку `{{имя}}`. */
  readonly left: string
  readonly op: 'contains' | 'not_contains' | 'equals' | 'not_equals' | 'empty' | 'not_empty'
  /** Правая часть; для `empty` и `not_empty` не нужна. */
  readonly right?: string
}

export interface Command {
  readonly id: string
  readonly name: string
  readonly description: string
  readonly trigger: Trigger
  readonly enabled: boolean
  readonly steps: readonly Step[]
}

/** Что произошло при выполнении — для журнала и для показа пользователю. */
export type RunEvent =
  | { readonly kind: 'step_started'; readonly index: number; readonly label: string }
  | {
      readonly kind: 'step_finished'
      readonly index: number
      readonly ok: boolean
      readonly detail: string
    }
  | { readonly kind: 'skipped'; readonly index: number; readonly reason: string }
  | { readonly kind: 'finished'; readonly steps: number }
  | { readonly kind: 'failed'; readonly index: number; readonly message: string }

export interface RunOutcome {
  readonly ok: boolean
  /** Сколько шагов выполнено. */
  readonly executed: number
  /** Значения, сохранённые шагами через `saveAs`. */
  readonly variables: Record<string, string>
  readonly error?: string
}
