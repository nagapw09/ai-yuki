/**
 * Обвязка проактивности: раз в минуту собирает обстановку, спрашивает правила
 * (`proactive.ts`) и, если есть что сказать, говорит — жестом, репликой в
 * чате и голосом, когда голос включён.
 */

import { invoke } from '@tauri-apps/api/core'

import { avatarPlay, commandList, companionContext, settingGet } from '../bridge'
import { useChatStore } from '../state/chatStore'
import { useUiStore } from '../state/store'
import { runById } from './commands'
import { INITIAL, step, type Nudge, type ProactiveState } from './proactive'
import { isYes } from './remote'
import { isTurnActive } from './session'
import { speakIfVoice } from './voice'

const TICK = 60_000
const STATE_KEY = 'yuki-proactive'
/** Сколько ждать «да» на предложение. */
const OFFER_TTL = 2 * 60_000

let started = false
let offer: { id: string; name: string; until: number } | null = null

function load(): ProactiveState {
  try {
    return { ...INITIAL, ...(JSON.parse(localStorage.getItem(STATE_KEY) ?? '{}') as Partial<ProactiveState>) }
  } catch {
    return INITIAL
  }
}

function save(state: ProactiveState): void {
  try {
    localStorage.setItem(STATE_KEY, JSON.stringify(state))
  } catch {
    // Без хранилища после перезапуска может повториться «доброе утро» — не беда.
  }
}

async function tick(): Promise<void> {
  if ((await settingGet('proactive.enabled').catch(() => null)) !== 'on') return

  const [context, info, commands] = await Promise.all([
    companionContext().catch(() => null),
    // Те же замеры, что на главной: память и батарея одним вызовом.
    invoke<{ memoryUsed: number; memoryTotal: number; battery: { percent: number; charging: boolean } | null }>('system_metrics').catch(() => null),
    commandList().catch(() => []),
  ])
  if (!context) return

  const orb = useUiStore.getState().orbState
  const { state, nudge } = step(
    load(),
    {
      now: new Date(),
      idleSeconds: context.idleSeconds,
      quiet: context.quiet ?? false,
      // «listening» — покой в режиме имени, а не занятость.
      busy: isTurnActive() || orb === 'speaking' || orb === 'thinking' || orb === 'working',
      memoryFree: info && info.memoryTotal > 0 ? 1 - info.memoryUsed / info.memoryTotal : null,
      battery: info?.battery ?? null,
    },
    commands,
  )
  save(state)
  if (nudge) await deliver(nudge)
}

async function deliver(nudge: Nudge): Promise<void> {
  if (nudge.motion) void avatarPlay(nudge.motion).catch(() => undefined)
  if (!nudge.text) return

  useChatStore.getState().note(nudge.text)
  useUiStore.getState().setHeadline(nudge.text)
  offer = nudge.offer ? { ...nudge.offer, until: Date.now() + OFFER_TTL } : null
  // После реплики голосом открывается окно разговора: «да» можно сказать без имени.
  await speakIfVoice(nudge.text)
}

/**
 * Ответ на предложение Yuki. Возвращает true, если сообщение было ответом и
 * дальше его передавать не нужно: «да» запускает команду, «нет» закрывает.
 */
export async function takeOffer(text: string): Promise<boolean> {
  const pending = offer
  if (!pending) return false
  offer = null
  if (Date.now() > pending.until) return false
  if (isYes(text)) {
    await runById(pending.id)
    return true
  }
  if (/^(нет|не надо|не нужно|потом|позже)(?=$|[\s,.!])/i.test(text.trim())) {
    useChatStore.getState().note('Хорошо.')
    void speakIfVoice('Хорошо.')
    return true
  }
  // Другая просьба — предложение молча снимается, просьба идёт своим путём.
  return false
}

export function startProactive(): void {
  if (started) return
  started = true
  setInterval(() => void tick().catch(() => undefined), TICK)
}
