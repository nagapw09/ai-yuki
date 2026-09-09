/**
 * Состояния аватара и их выражение на лице (ТЗ §12).
 *
 * Восемь состояний заданы ТЗ и совпадают с состояниями Orb из ТЗ §13 — это
 * один и тот же автомат, показанный двумя способами. Поэтому отдельного
 * перечисления здесь нет: аватар подписывается на то же состояние, что и Orb,
 * и переводит его в мимику.
 */

import type { OrbState } from '../state/types'

/** Как выглядит одно состояние. */
export interface StateLook {
  /** Имя выражения VRM (VRMExpressionPresetName). */
  expression: 'neutral' | 'happy' | 'angry' | 'sad' | 'relaxed' | 'surprised'
  /** Насколько сильно оно проявлено, 0…1. */
  weight: number
  /** Амплитуда покачивания корпуса — «живость» позы. */
  sway: number
  /** Частота дыхания, вдохов в секунду. */
  breath: number
  /** Говорит ли аватар: включает движение рта. */
  speaking: boolean
  /** Спит ли: глаза закрыты, дыхание медленное. */
  asleep: boolean
}

/**
 * Соответствие состояния и мимики.
 *
 * THINKING и WORKING различаются не выражением, а темпом: думающий аватар
 * почти неподвижен, работающий — заметно живее. Если бы они отличались только
 * лицом, разница потерялась бы на маленьком окне.
 */
export const LOOKS: Record<OrbState, StateLook> = {
  idle: {
    expression: 'neutral',
    weight: 0,
    sway: 1,
    breath: 0.25,
    speaking: false,
    asleep: false,
  },
  listening: {
    // Чуть приподнятое лицо: слушающий человек не хмурится и не улыбается.
    expression: 'relaxed',
    weight: 0.3,
    sway: 0.6,
    breath: 0.3,
    speaking: false,
    asleep: false,
  },
  thinking: {
    expression: 'neutral',
    weight: 0,
    sway: 0.3,
    breath: 0.2,
    speaking: false,
    asleep: false,
  },
  working: {
    expression: 'neutral',
    weight: 0,
    sway: 1.6,
    breath: 0.5,
    speaking: false,
    asleep: false,
  },
  speaking: {
    expression: 'happy',
    weight: 0.25,
    sway: 1.1,
    breath: 0.35,
    speaking: true,
    asleep: false,
  },
  success: {
    expression: 'happy',
    weight: 1,
    sway: 1.8,
    breath: 0.4,
    speaking: false,
    asleep: false,
  },
  error: {
    // Не злость, а огорчение: ошибка Yuki — повод извиниться, а не хмуриться
    // на пользователя.
    expression: 'sad',
    weight: 0.8,
    sway: 0.4,
    breath: 0.25,
    speaking: false,
    asleep: false,
  },
  sleeping: {
    expression: 'neutral',
    weight: 0,
    sway: 0.2,
    breath: 0.12,
    speaking: false,
    asleep: true,
  },
}

/** Все выражения, которыми пользуется аватар: их надо гасить при смене состояния. */
export const ALL_EXPRESSIONS = [
  'neutral',
  'happy',
  'angry',
  'sad',
  'relaxed',
  'surprised',
] as const
