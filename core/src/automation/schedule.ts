/**
 * Расписание команд (ТЗ §16: triggers).
 *
 * Хранится строкой «ЧЧ:ММ|дни»: «09:00|1,2,3,4,5» — по будням в девять,
 * «22:30» — каждый день. Дни — 1 (понедельник) … 7 (воскресенье).
 */

export interface Schedule {
  readonly hour: number
  readonly minute: number
  /** Пустой список — каждый день. */
  readonly days: readonly number[]
}

/**
 * Сколько минут после назначенного времени команду ещё можно выполнить.
 * Компьютер мог спать ровно в 09:00 — проснувшись в 09:07, утренний сценарий
 * всё ещё уместен, а в полдень уже нет.
 */
export const SCHEDULE_GRACE_MINUTES = 15

export function parseSchedule(text: string | null | undefined): Schedule | null {
  if (!text) return null
  const [time, days = ''] = text.split('|')
  const match = /^\s*(\d{1,2}):(\d{2})\s*$/.exec(time ?? '')
  if (!match) return null
  const hour = Number(match[1])
  const minute = Number(match[2])
  if (hour > 23 || minute > 59) return null
  const list = days
    .split(',')
    .map((d) => d.trim())
    .filter(Boolean)
    .map(Number)
  if (list.some((d) => !Number.isInteger(d) || d < 1 || d > 7)) return null
  return { hour, minute, days: [...new Set(list)].sort() }
}

export function formatSchedule(schedule: Schedule): string {
  const time = `${String(schedule.hour).padStart(2, '0')}:${String(schedule.minute).padStart(2, '0')}`
  return schedule.days.length ? `${time}|${schedule.days.join(',')}` : time
}

/** 1 — понедельник … 7 — воскресенье. */
export function isoWeekday(date: Date): number {
  return ((date.getDay() + 6) % 7) + 1
}

/** Ключ дня по местному времени: одна команда — один запуск в день. */
export function dayKey(date: Date): string {
  const m = String(date.getMonth() + 1).padStart(2, '0')
  const d = String(date.getDate()).padStart(2, '0')
  return `${date.getFullYear()}-${m}-${d}`
}

/**
 * Пора ли выполнить команду. `lastDay` — день прошлого запуска по расписанию
 * (`dayKey`), чтобы проверка раз в несколько секунд не запускала её повторно.
 */
export function isDue(schedule: Schedule, now: Date, lastDay: string | null): boolean {
  if (lastDay === dayKey(now)) return false
  if (schedule.days.length && !schedule.days.includes(isoWeekday(now))) return false
  const late = now.getHours() * 60 + now.getMinutes() - (schedule.hour * 60 + schedule.minute)
  return late >= 0 && late < SCHEDULE_GRACE_MINUTES
}

const WEEKDAYS = ['пн', 'вт', 'ср', 'чт', 'пт', 'сб', 'вс']

/** «по будням в 09:00», «каждый день в 22:30», «пн, ср в 18:00». */
export function describeSchedule(schedule: Schedule): string {
  const time = formatSchedule({ ...schedule, days: [] })
  const days = schedule.days.join(',')
  const when =
    !schedule.days.length || days === '1,2,3,4,5,6,7'
      ? 'каждый день'
      : days === '1,2,3,4,5'
        ? 'по будням'
        : days === '6,7'
          ? 'по выходным'
          : schedule.days.map((d) => WEEKDAYS[d - 1]).join(', ')
  return `${when} в ${time}`
}
