import { describe, expect, it } from 'vitest'

import type { CommandRecord } from '../bridge'
import { INITIAL, step, type Moment, type ProactiveState } from './proactive'

const at = (h: number, m = 0, day = 24) => new Date(2026, 8, day, h, m)
const moment = (now: Date, extra: Partial<Moment> = {}): Moment => ({
  now,
  idleSeconds: 5,
  quiet: false,
  busy: false,
  memoryFree: 0.5,
  ...extra,
})
const work: CommandRecord = {
  id: 'w',
  name: 'Рабочий режим',
  description: '',
  triggerKind: 'phrase',
  phrase: 'рабочий режим',
  hotkey: null,
  enabled: true,
  steps: [{}],
}

/** Прогоняет минуты подряд, возвращает всё сказанное. */
function run(state: ProactiveState, from: Date, minutes: number, extra: Partial<Moment> = {}) {
  const said: string[] = []
  for (let i = 0; i < minutes; i++) {
    const r = step(state, moment(new Date(from.getTime() + i * 60_000), extra), [work])
    state = r.state
    if (r.nudge) said.push(r.nudge.kind)
  }
  return { state, said }
}

describe('proactive', () => {
  it('greets once in the morning and offers the work command', () => {
    const r = step(INITIAL, moment(at(9)), [work])
    expect(r.nudge).toMatchObject({ kind: 'morning', offer: { id: 'w' } })
    expect(r.nudge!.text).toContain('Рабочий режим')
    expect(step(r.state, moment(at(9, 5)), [work]).nudge).toBeNull()
  })

  it('stays silent while a game is fullscreen, then greets', () => {
    const quiet = step(INITIAL, moment(at(9), { quiet: true }), [work])
    expect(quiet.nudge).toBeNull()
    expect(step(quiet.state, moment(at(9, 1)), [work]).nudge?.kind).toBe('morning')
  })

  it('asks for a break after two hours in a row, not before', () => {
    const { said } = run(INITIAL, at(13), 125)
    expect(said).toEqual(['break'])
  })

  it('a ten-minute pause resets the two hours', () => {
    let { state } = run(INITIAL, at(13), 100)
    state = step(state, moment(at(14, 45), { idleSeconds: 15 * 60 }), []).state
    const after = run(state, at(15), 60)
    expect(after.said).not.toContain('break')
  })

  it('waves without words when the person comes back', () => {
    let { state } = run(INITIAL, at(13), 5)
    state = step(state, moment(at(14), { idleSeconds: 50 * 60 }), []).state
    const back = step(state, moment(at(14, 1), { idleSeconds: 3 }), [])
    expect(back.nudge).toMatchObject({ kind: 'welcome', text: '' })
  })

  it('nags about sleep once a night', () => {
    const { said } = run(INITIAL, at(1, 30), 60)
    expect(said).toEqual(['late'])
  })

  it('warns when memory stays low', () => {
    const { said } = run({ ...INITIAL, greetedDay: '2026-9-24' }, at(15), 5, { memoryFree: 0.04 })
    expect(said).toEqual(['memory'])
  })
})

describe('battery', () => {
  it('warns once below 15% until charged, even in fullscreen', () => {
    const low = { battery: { percent: 12, charging: false }, quiet: true }
    const first = step({ ...INITIAL, greetedDay: '2026-9-24' }, moment(at(15), low), [])
    expect(first.nudge?.kind).toBe('battery')
    expect(step(first.state, moment(at(15, 1), low), []).nudge).toBeNull()
    const charged = step(first.state, moment(at(15, 2), { battery: { percent: 13, charging: true } }), []).state
    expect(step(charged, moment(at(15, 3), low), []).nudge?.kind).toBe('battery')
  })
})
