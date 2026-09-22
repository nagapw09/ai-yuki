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
  voiceSpeakingLevel,
  voiceStart,
  voiceStop,
  voiceStatus,
  voiceStopSpeaking,
  voiceInputName,
  type ListenMode,
} from '../bridge'
import { useUiStore } from '../state/store'
import { speakable } from './reply'
import { recognize } from './quick'
import { cancelCurrentTurn, isTurnActive, sendMessage } from './session'

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
 * Слежение за микрофоном по умолчанию.
 *
 * Захват открывает устройство один раз, при включении. Подключённые потом
 * наушники становились микрофоном системы, а Yuki продолжала слушать
 * встроенный — человек говорил в наушники, и его не было слышно. Теперь смена
 * устройства перезапускает прослушивание в том же режиме.
 */
let deviceWatch: ReturnType<typeof setInterval> | null = null
const DEVICE_POLL_MS = 3000

function watchDevice(mode: ListenMode, device: string | null) {
  stopDeviceWatch()
  let current = device
  let restarting = false
  deviceWatch = setInterval(() => {
    if (restarting) return
    void voiceInputName().then(async (name) => {
      if (!name || name === current || !isVoiceActive()) return
      restarting = true
      current = name
      try {
        await stopVoice()
        await startVoice(mode)
        useUiStore.getState().setHeadline(`Слушаю: ${name}`)
      } catch {
        /* startVoice уже показал причину */
      } finally {
        restarting = false
      }
    }).catch(() => undefined)
  }, DEVICE_POLL_MS)
}

function stopDeviceWatch() {
  if (deviceWatch) clearInterval(deviceWatch)
  deviceWatch = null
}

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

/** Что ответить на одно лишь имя. */
const REPLIES = ['Да?', 'Слушаю.', 'Да-да?', 'М?']

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
  const spoken = speakable(text)
  if (!speakReplies || !spoken) return

  try {
    await voiceSpeak(spoken)
  } catch {
    return
  }

  // Громкость опрашивается чаще, чем состояние: рот аватара должен идти за
  // звуком, а не за фактом «говорит». Если движок громкости не отдаёт, опрос
  // сам прекращается — тогда рот работает по ритму слогов, как раньше.
  void followLevel()

  useUiStore.getState().setOrbState('speaking')

  while (speakReplies) {
    await new Promise((resolve) => setTimeout(resolve, SPEAKING_POLL_MS))
    const status = await voiceStatus().catch(() => null)
    if (!status?.speaking) break
  }

  useUiStore.getState().setAudioLevel(0)

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
      // Оклик без команды: отзываемся вслух, как Джарвис. Пока она говорит
      // «Да?», микрофон её не слушает, а после ответа открывается окно
      // продолжения — просьбу можно договорить без имени.
      useUiStore.getState().setHeadline('Слушаю')
      if (!isTurnActive()) void speakIfVoice(REPLIES[Math.floor(Math.random() * REPLIES.length)]!)
      return
    }

    // Пользователь заговорил — Yuki замолкает, даже если ещё отвечает (ТЗ §10).
    // Экран не переключаем: голосом говорят, не глядя в окно, и прыгающий
    // интерфейс только мешает. Ход виден в чате, когда его откроют.
    void voiceStopSpeaking().catch(() => undefined)

    // «Стоп» должен работать и пока Yuki думает: иначе он ждал бы конца хода,
    // который как раз и просили прервать.
    if (recognize(said)?.kind === 'stop') {
      cancelCurrentTurn()
      return
    }
    // Новая просьба посреди хода раньше пропадала молча. Теперь человек хотя
    // бы видит, что его услышали, но заняты прошлым.
    if (isTurnActive()) {
      useUiStore.getState().setHeadline('Секунду, заканчиваю прошлое')
      return
    }
    void sendMessage(said).catch(() => undefined)
  })

  unlisteners = [level, state, text]
  speakReplies = true

  try {
    await voiceStart(mode)
    watchDevice(mode, await voiceInputName().catch(() => null))
    ui.setListenMode(mode)
    ui.setVoiceActive(true)
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
  stopDeviceWatch()
  for (const off of unlisteners) off()
  unlisteners = []
  speakReplies = false
  useUiStore.getState().setListenMode(null)
  useUiStore.getState().setVoiceActive(false)

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

/**
 * Гонит громкость речи в состояние, пока Yuki говорит.
 *
 * Шестьдесят миллисекунд — это примерно шестнадцать замеров в секунду: реже
 * рот отстаёт от звука заметно, чаще — незачем, потому что и сама громкость
 * считается по кускам буфера такого же порядка.
 */
async function followLevel(): Promise<void> {
  const store = useUiStore.getState()

  for (;;) {
    const level = await voiceSpeakingLevel().catch(() => null)

    // `null` означает, что движок звука не отдаёт: продолжать опрос нечего.
    if (level === null) return

    store.setAudioLevel(level)

    // Ноль после начала речи бывает в паузах между словами, поэтому выходим
    // не по нему, а по состоянию синтезатора.
    const status = await voiceStatus().catch(() => null)
    if (!status?.speaking) {
      store.setAudioLevel(0)
      return
    }

    await new Promise((resolve) => setTimeout(resolve, 60))
  }
}
