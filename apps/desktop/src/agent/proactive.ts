/**
 * Проактивность (ТЗ §34): Yuki сама заводит разговор.
 *
 * Питомец, а не будильник: здоровается утром, машет, когда человек вернулся,
 * зовёт размяться после двух часов подряд и ворчит, что пора спать. Всё это —
 * правила без модели: раз в минуту одна проверка, ни токенов, ни нагрузки.
 *
 * Включается в «Персонаж → Сама заводит разговор» (по ТЗ — opt-in). Молчит,
 * пока идёт игра или видео на весь экран, пока занята ответом и чаще, чем раз
 * в полчаса. Предложить может только команду пользователя, и выполняется она
 * через тот же Permission Gate, что и по фразе.
 */

import type { CommandRecord } from '../bridge'

export interface Moment {
  readonly now: Date
  readonly idleSeconds: number
  /** Полноэкранная игра или видео, презентация, заблокированный экран. */
  readonly quiet: boolean
  /** Yuki думает, выполняет или говорит. */
  readonly busy: boolean
  /** Доля свободной памяти, 0…1; null — неизвестно. */
  readonly memoryFree: number | null
}

export interface ProactiveState {
  /** С какого момента человек непрерывно за компьютером. */
  readonly activeSince: number | null
  /** Когда его последний раз видели за компьютером. */
  readonly lastActive: number | null
  readonly lastSpoken: number
  readonly greetedDay: string | null
  readonly lateDay: string | null
  readonly lastBreak: number
  readonly lowMemoryTicks: number
  readonly lastMemory: number
}

export type NudgeKind = 'morning' | 'welcome' | 'break' | 'late' | 'memory'

export interface Nudge {
  readonly kind: NudgeKind
  /** Что сказать; пусто — только жест персонажа. */
  readonly text: string
  readonly motion: string | null
  /** Команда, которую Yuki предлагает запустить: «да» её выполнит. */
  readonly offer: { readonly id: string; readonly name: string } | null
}

export const INITIAL: ProactiveState = {
  activeSince: null,
  lastActive: null,
  lastSpoken: 0,
  greetedDay: null,
  lateDay: null,
  lastBreak: 0,
  lowMemoryTicks: 0,
  lastMemory: 0,
}

const MIN = 60_000
const HOUR = 60 * MIN
/** Без мыши и клавиатуры дольше — человек отошёл. */
const ACTIVE_IDLE_S = 120
/** Столько не было — перерыв засчитан, счёт «двух часов» с нуля. */
const BREAK_IDLE_S = 10 * 60
const AWAY_FOR_WELCOME = 30 * MIN
const SPEAK_COOLDOWN = 30 * MIN
const BREAK_AFTER = 2 * HOUR
const MEMORY_LOW = 0.08

function dayKey(date: Date): string {
  return `${date.getFullYear()}-${date.getMonth() + 1}-${date.getDate()}`
}

/** Команда, похожая на «рабочий режим» или утренний набор. */
export function morningCommand(commands: readonly CommandRecord[]): CommandRecord | null {
  return (
    commands.find(
      (c) =>
        c.enabled &&
        c.steps.length > 0 &&
        // Сработает сама — предлагать незачем.
        c.triggerKind !== 'schedule' &&
        c.triggerKind !== 'startup' &&
        /рабоч|утр|work|morning/i.test(`${c.name} ${c.phrase ?? ''}`),
    ) ?? null
  )
}

function hoursWord(n: number): string {
  if (n === 1) return 'час'
  if (n >= 2 && n <= 4) return `${['', '', 'два', 'три', 'четыре'][n]} часа`
  return `${n} часов`
}

/**
 * Один шаг проверки: новое состояние и, может быть, что сказать.
 *
 * Чистая функция — время, простой и память приходят снаружи, поэтому правила
 * проверяются тестами без ожидания двух часов.
 */
export function step(
  state: ProactiveState,
  moment: Moment,
  commands: readonly CommandRecord[],
): { state: ProactiveState; nudge: Nudge | null } {
  const t = moment.now.getTime()
  const hour = moment.now.getHours()
  const today = dayKey(moment.now)
  const active = moment.idleSeconds < ACTIVE_IDLE_S

  let next: ProactiveState = state
  if (!active) {
    if (moment.idleSeconds >= BREAK_IDLE_S) next = { ...next, activeSince: null }
    return { state: next, nudge: null }
  }

  // Время последней активности считаем по простою: минутный опрос мог
  // пропустить сам момент возвращения.
  const cameBack = t - moment.idleSeconds * 1000
  const away = state.lastActive === null ? 0 : cameBack - state.lastActive
  next = {
    ...next,
    activeSince: state.activeSince ?? cameBack,
    lastActive: t,
  }

  // Память проверяется всегда: предупреждение важнее кулдауна.
  const lowMemory = moment.memoryFree !== null && moment.memoryFree < MEMORY_LOW
  next = { ...next, lowMemoryTicks: lowMemory ? state.lowMemoryTicks + 1 : 0 }

  const canSpeak = !moment.quiet && !moment.busy
  const rested = t - state.lastSpoken >= SPEAK_COOLDOWN
  const say = (nudge: Nudge, patch: Partial<ProactiveState>) => ({
    state: { ...next, ...patch, lastSpoken: nudge.text ? t : next.lastSpoken },
    nudge,
  })

  if (!canSpeak) return { state: next, nudge: null }

  if (next.lowMemoryTicks >= 3 && t - state.lastMemory >= 2 * HOUR) {
    const free = Math.round((moment.memoryFree ?? 0) * 100)
    return say(
      {
        kind: 'memory',
        text: `Памяти почти не осталось — свободно ${free}%. Закройте что-нибудь лишнее, а то всё начнёт тормозить.`,
        motion: null,
        offer: null,
      },
      { lastMemory: t },
    )
  }

  if (state.greetedDay !== today && hour >= 5 && hour < 12) {
    const command = morningCommand(commands)
    return say(
      {
        kind: 'morning',
        text: command ? `Доброе утро! Запустить «${command.name}»?` : 'Доброе утро!',
        motion: 'builtin:wave',
        offer: command ? { id: command.id, name: command.name } : null,
      },
      { greetedDay: today },
    )
  }

  if (away >= AWAY_FOR_WELCOME) {
    // Только жест: вслух «с возвращением» после каждого обеда надоест.
    return { state: next, nudge: { kind: 'welcome', text: '', motion: 'builtin:wave', offer: null } }
  }

  if (!rested) return { state: next, nudge: null }

  if (hour >= 1 && hour < 5 && state.lateDay !== today && t - (next.activeSince ?? t) >= 20 * MIN) {
    return say(
      {
        kind: 'late',
        text: `Уже ${hour === 1 ? 'второй' : hour === 2 ? 'третий' : hour === 3 ? 'четвёртый' : 'пятый'} час ночи. Может, пора спать?`,
        motion: 'builtin:stretch',
        offer: null,
      },
      { lateDay: today },
    )
  }

  const sitting = t - (next.activeSince ?? t)
  if (sitting >= BREAK_AFTER && t - state.lastBreak >= BREAK_AFTER) {
    const hours = Math.floor(sitting / HOUR)
    return say(
      {
        kind: 'break',
        text: `Вы уже ${hoursWord(hours)} за компьютером. Давайте немного разомнёмся?`,
        motion: 'builtin:stretch',
        offer: null,
      },
      { lastBreak: t },
    )
  }

  return { state: next, nudge: null }
}
