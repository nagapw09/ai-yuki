import type { OrbState } from '../state/types'
import { advance, choose, cursorCloseness, noticed, DEFAULT_VITALS, type Vitals } from './character'

export type BehaviorMode = 'calm' | 'playful' | 'quiet'
export interface CompanionContext { musicPlaying: boolean; idleSeconds: number; mediaAvailable: boolean }
export interface BehaviorInput extends CompanionContext {
  state: OrbState
  mode: BehaviorMode
  musicEnabled: boolean
  canWalk: boolean
  availableActions?: readonly string[]
  /** `listening` — это дежурство по слову пробуждения, а не разговор. */
  passive?: boolean
  /** Курсор в долях окна, 0…1 по каждой оси. Вне окна — не передавать. */
  cursor?: { x: number; y: number }
}

/**
 * Приоритеты занятий.
 *
 * Прерывать можно только то, что менее важно. Без этого просьба человека
 * терялась под очередным самостоятельным занятием, а системное состояние —
 * под просьбой.
 */
export const PRIORITY = { system: 100, user: 80, ai: 50, idle: 10 } as const

/**
 * Повседневное поведение компаньона.
 *
 * Никаких запросов к языковой модели и никакого чтения экрана: чем занять себя
 * в тишине, персонаж решает сам по своим внутренним величинам (см. `character.ts`).
 * Модель отвечает за разговор, а не за то, куда ему смотреть каждую секунду.
 */
export class CompanionBehavior {
  action = 'builtin:stand'
  vitals: Vitals = { ...DEFAULT_VITALS }
  private next = 0
  private manualUntil = 0
  private interrupted = false
  private musicDance = false
  private lastTick = 0
  private priority: number = PRIORITY.idle
  constructor(private random: () => number = Math.random) {}

  /** Подставляет сохранённые величины при запуске. */
  restore(vitals: Vitals) {this.vitals = vitals}

  manual(action: string, now: number) {
    if (action.startsWith('emotion:')) return
    this.action = action || 'builtin:stand'
    this.priority = PRIORITY.user
    this.manualUntil = now + 120
    this.next = this.manualUntil
    this.musicDance = false
    // Просьба человека — это внимание к персонажу.
    this.vitals = noticed(this.vitals, 0.02)
  }

  /** Человек потрогал персонажа: внимание дороже любого занятия. */
  touched(strength = 0.06) {this.vitals = noticed(this.vitals, strength)}

  resume(now: number) {this.manualUntil = 0; this.next = now; this.priority = PRIORITY.idle}

  tick(now: number, input: BehaviorInput): string | null {
    const available = (action: string) => !input.availableActions || input.availableActions.includes(action)

    // Величины идут своим ходом независимо от того, вернём мы занятие или нет.
    const delta = this.lastTick ? Math.max(0, now - this.lastTick) : 0
    this.lastTick = now
    const cursor = input.cursor ? cursorCloseness(input.cursor.x, input.cursor.y) : 0
    this.vitals = advance(this.vitals, {
      delta,
      idleSeconds: input.idleSeconds,
      cursor,
      sleeping: this.action === 'builtin:sleep',
      busy: this.action !== 'builtin:stand' && this.action !== 'builtin:sit',
    })

    // Дежурство по слову пробуждения — не разговор. Раньше при включённом
    // ожидании имени микрофон открыт постоянно, состояние всегда `listening`,
    // и компаньон навсегда застывал «во внимании», не занимаясь ничем.
    const listeningIdly = input.state === 'listening' && input.passive === true
    const attentive = input.state !== 'idle' && input.state !== 'sleeping' && !listeningIdly

    if (attentive) {
      this.next = now + 12
      // Разговор — это внимание: рядом с человеком персонаж не скучает.
      this.vitals = noticed(this.vitals, delta * 0.002)
      if (!this.interrupted && this.action !== 'builtin:sit' && this.action !== 'builtin:stand') {
        this.interrupted = true; return this.set('builtin:stand', PRIORITY.system)
      }
      return null
    }
    this.interrupted = false
    if (now < this.manualUntil || input.mode === 'quiet') return null

    if (input.musicEnabled && input.musicPlaying && available('builtin:dance')) {
      if (!this.musicDance) {this.musicDance = true; return this.set('builtin:dance', PRIORITY.ai)}
      return null
    }
    if (this.musicDance) {this.musicDance = false; this.next = now + 15; return this.set('builtin:stand', PRIORITY.idle)}

    // Сон — отдельное устойчивое состояние: из него выходят потягиванием, а не
    // очередным жребием.
    if (this.action === 'builtin:sleep') {
      if (this.vitals.energy < 0.9 && input.idleSeconds > 600) return null
      this.next = now + 7
      return this.set(available('builtin:stretch') ? 'builtin:stretch' : 'builtin:stand', PRIORITY.idle)
    }

    if (now < this.next) return null

    const playful = input.mode === 'playful'
    const next = choose({
      vitals: this.vitals,
      idleSeconds: input.idleSeconds,
      cursor,
      music: false,
      canWalk: input.canWalk,
      current: this.action,
      available,
    }, this.random)

    // Живой режим меняет занятия чаще, спокойный — реже.
    const pause = next === 'builtin:walk' ? 7 + this.random() * 8
      : next === 'builtin:stretch' || next === 'builtin:wave' ? 6
      : next === 'builtin:sleep' ? 60
      : (playful ? 15 : 20) + this.random() * 20
    this.next = now + pause
    // Смена занятия разгоняет скуку.
    this.vitals = {...this.vitals, boredom: Math.max(0, this.vitals.boredom - 0.15)}
    return this.set(next, PRIORITY.idle)
  }

  private set(action: string, priority: number) {this.action = action; this.priority = priority; return action}
}
