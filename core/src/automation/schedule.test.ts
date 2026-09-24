import { describe, expect, it } from 'vitest'

import { dayKey, describeSchedule, formatSchedule, isDue, parseSchedule } from './schedule'

// 2026-09-21 — понедельник.
const at = (day: number, h: number, m: number) => new Date(2026, 8, day, h, m)

describe('parseSchedule', () => {
  it('reads time and weekdays', () => {
    expect(parseSchedule('09:00|1,2,3,4,5')).toEqual({ hour: 9, minute: 0, days: [1, 2, 3, 4, 5] })
    expect(parseSchedule('7:05')).toEqual({ hour: 7, minute: 5, days: [] })
    expect(formatSchedule(parseSchedule('7:05|3,1')!)).toBe('07:05|1,3')
  })

  it('rejects nonsense', () => {
    expect(parseSchedule('24:00')).toBeNull()
    expect(parseSchedule('утром')).toBeNull()
    expect(parseSchedule('09:00|8')).toBeNull()
    expect(parseSchedule(null)).toBeNull()
  })
})

describe('isDue', () => {
  const weekdays = parseSchedule('09:00|1,2,3,4,5')!

  it('fires at the time and a little after, once a day', () => {
    expect(isDue(weekdays, at(21, 8, 59), null)).toBe(false)
    expect(isDue(weekdays, at(21, 9, 0), null)).toBe(true)
    expect(isDue(weekdays, at(21, 9, 10), null)).toBe(true)
    expect(isDue(weekdays, at(21, 9, 20), null)).toBe(false)
    expect(isDue(weekdays, at(21, 9, 1), dayKey(at(21, 9, 0)))).toBe(false)
    expect(isDue(weekdays, at(22, 9, 1), dayKey(at(21, 9, 0)))).toBe(true)
  })

  it('skips days not in the list', () => {
    expect(isDue(weekdays, at(26, 9, 0), null)).toBe(false) // суббота
    expect(isDue(parseSchedule('09:00')!, at(26, 9, 0), null)).toBe(true)
  })
})

describe('describeSchedule', () => {
  it('speaks like a person', () => {
    expect(describeSchedule(parseSchedule('09:00|1,2,3,4,5')!)).toBe('по будням в 09:00')
    expect(describeSchedule(parseSchedule('22:30')!)).toBe('каждый день в 22:30')
    expect(describeSchedule(parseSchedule('18:00|1,3')!)).toBe('пн, ср в 18:00')
  })
})
