/**
 * Голосовой режим в приложении (ТЗ §10).
 *
 * Конвейер целиком в Rust; здесь — связь с интерфейсом и одно продуктовое
 * решение, которого в конвейере быть не может: что делать с распознанной фразой.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import {
  voiceFinishUtterance,
  voiceSpeak,
  voiceStart,
  voiceStop,
  voiceStatus,
  voiceStopSpeaking,
  type ListenMode,
} from '../bridge'
import { useUiStore } from '../state/store'
import { sendMessage } from './session'

interface LevelEvent {
  level: number
}

interface StateEvent {
  state: 'listening' | 'speech' | 'addressed' | 'transcribing' | 'idle' | 'error'
  message: string | null
}

interface TranscribedEvent {
  text: string
  addressed: boolean
}

/** Подписки живут, пока включён голосовой режим. */
let unlisteners: UnlistenFn[] = []

/**
 * Озвучивать ли ответы.
 *
 * Голос включают, чтобы разговаривать, поэтому TTS следует за режимом
 * прослушивания: набранный текст Yuki не зачитывает, произнесённый — да.
 */
let speakReplies = false

/** Включён ли голосовой режим. */
export function isVoiceActive(): boolean {
  return unlisteners.length > 0
}

/** Как часто спрашиваем движок, договорил ли он. */
const SPEAKING_POLL_MS = 250

/**
 * Озвучивает ответ, если разговор идёт голосом, и держит Orb в состоянии
 * SPEAKING, пока движок действительно говорит.
 *
 * Состояние снимается по опросу движка, а не по оценке длины текста: скорость
 * речи зависит от голоса и настроек, и «примерно столько секунд» разошлось бы
 * с реальностью на первой же длинной фразе.
 */
export async function speakIfVoice(text: string): Promise<void> {
  if (!speakReplies || !text.trim()) return

  try {
    await voiceSpeak(text)
  } catch {
    return
  }

  useUiStore.getState().setOrbState('speaking')

  while (speakReplies) {
    await new Promise((resolve) => setTimeout(resolve, SPEAKING_POLL_MS))
    const status = await voiceStatus().catch(() => null)
    if (!status?.speaking) break
  }

  // Режим мог выключиться, пока Yuki говорила: тогда состояние Orb уже задано.
  if (speakReplies) useUiStore.getState().setOrbState('listening')
}

/**
 * Включает голосовой режим.
 *
 * `push_to_talk` слушает до явной остановки, `wake_word` — постоянно, реагируя
 * только на обращение к Yuki.
 */
export async function startVoice(mode: ListenMode): Promise<void> {
  if (isVoiceActive()) return

  const ui = useUiStore.getState()

  const level = await listen<LevelEvent>('yuki://voice-level', (event) => {
    useUiStore.getState().setAudioLevel(event.payload.level)
  })

  const state = await listen<StateEvent>('yuki://voice-state', (event) => {
    const store = useUiStore.getState()
    switch (event.payload.state) {
      case 'listening':
      case 'speech':
        store.setOrbState('listening')
        break
      // Отзыв на собственное имя (ТЗ §37): приходит в момент произнесения
      // слова, пока человек ещё говорит фразу. В этом и смысл бюджета:
      // видимый ответ до конца реплики.
      case 'addressed':
        store.setOrbState('listening')
        store.setHeadline('Слушаю')
        break
      case 'transcribing':
        store.setOrbState('thinking')
        break
      case 'idle':
        store.setOrbState('idle')
        store.setAudioLevel(0)
        break
      case 'error':
        store.setHeadline(event.payload.message ?? 'Не удалось распознать речь')
        store.flashResult('error')
        break
    }
  })

  const text = await listen<TranscribedEvent>('yuki://voice-transcribed', (event) => {
    // В режиме слова пробуждения всё, что сказано не Yuki, — фон комнаты.
    // Показывать его или как-то реагировать значит подслушивать.
    if (!event.payload.addressed) return

    const said = event.payload.text.trim()
    if (!said) {
      // Оклик без команды: отзываемся, а не молчим.
      useUiStore.getState().setHeadline('Слушаю')
      return
    }

    // Пользователь заговорил — Yuki замолкает, даже если ещё отвечает (ТЗ §10).
    void voiceStopSpeaking().catch(() => undefined)
    useUiStore.getState().setScreen('chat')
    void sendMessage(said).catch(() => undefined)
  })

  unlisteners = [level, state, text]
  speakReplies = true

  try {
    await voiceStart(mode)
    ui.setOrbState('listening')
  } catch (error) {
    // Не удалось открыть микрофон — снимаем подписки, чтобы состояние не
    // осталось «слушаю» при выключенном захвате.
    await stopVoice()
    ui.setHeadline(describe(error))
    ui.flashResult('error')
    throw error
  }
}

/** Выключает голосовой режим и замолкает. */
export async function stopVoice(): Promise<void> {
  for (const off of unlisteners) off()
  unlisteners = []
  speakReplies = false

  await voiceStopSpeaking().catch(() => undefined)
  await voiceStop().catch(() => undefined)

  const ui = useUiStore.getState()
  ui.setAudioLevel(0)
  ui.setOrbState('idle')
}

/** Отпущена кнопка push-to-talk. */
export async function finishUtterance(): Promise<void> {
  if (!isVoiceActive()) return
  await voiceFinishUtterance().catch(() => undefined)
}

function describe(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return 'Не удалось включить микрофон'
}
