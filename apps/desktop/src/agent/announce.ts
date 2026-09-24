/**
 * Напоминание вслух (ТЗ §25).
 *
 * Карточка и уведомление Windows легко пропустить в наушниках посреди фильма
 * или игры. Поэтому напоминание Yuki произносит: если что-то играет — ставит
 * на паузу, говорит и снимает паузу обратно. Игру поставить на паузу нельзя,
 * но и тогда она говорит: напоминание человек заказал сам, и «не мешать»
 * здесь значило бы его не выполнить.
 */

import { listen } from '@tauri-apps/api/event'

import { avatarPlay, mediaControl, mediaNowPlaying, voiceSpeak, voiceStatus } from '../bridge'

/** Дольше ни одна фраза напоминания не звучит — дальше не ждём и продолжаем. */
const SPEECH_LIMIT_MS = 30_000
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

let queue: Promise<void> = Promise.resolve()

async function waitSpeechEnd(): Promise<void> {
  const until = Date.now() + SPEECH_LIMIT_MS
  // Движок начинает говорить не сразу после вызова.
  await sleep(600)
  while (Date.now() < until) {
    const status = await voiceStatus().catch(() => null)
    if (!status?.speaking) return
    await sleep(300)
  }
}

export async function announce(text: string): Promise<void> {
  const now = await mediaNowPlaying().catch(() => null)
  const paused = !!(now?.available && now.playing && now.canPause)
  if (paused) {
    await mediaControl('pause').catch(() => undefined)
    // Плеер затихает не мгновенно: иначе первые слова тонут в музыке.
    await sleep(400)
  }
  void avatarPlay('builtin:wave').catch(() => undefined)
  try {
    await voiceSpeak(`Напоминаю: ${text}`)
    await waitSpeechEnd()
  } catch {
    // Голоса нет — остаются карточка и уведомление Windows.
  }
  if (paused) await mediaControl('play').catch(() => undefined)
}

/** Подписка на сработавшие напоминания. Несколько подряд звучат по очереди. */
export function startAnnouncer(): void {
  void listen<{ text: string }>('yuki://reminder-fired', (event) => {
    const text = event.payload?.text?.trim()
    if (!text) return
    queue = queue.then(() => announce(text)).catch(() => undefined)
  })
}
