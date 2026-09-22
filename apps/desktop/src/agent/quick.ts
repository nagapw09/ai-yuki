/**
 * Быстрые команды без языковой модели.
 *
 * Идея взята у Priler/jarvis: частые просьбы узнаются по фразе и выполняются
 * сразу. Модель через CLI отвечает за несколько секунд — для «открой браузер»
 * это вечность, а для «стоп» ещё и бессмыслица: пока модель думает, Yuki
 * продолжает говорить.
 *
 * Всё, что здесь не узнано уверенно, уходит модели. Лучше лишний раз подумать,
 * чем открыть не то приложение.
 */

import { evaluate, type Message } from '@yuki/core'

import { avatarPlay, activityRecord, mediaControl, voiceStopSpeaking } from '../bridge'
import { useChatStore } from '../state/chatStore'
import { useUiStore } from '../state/store'
import { gateSettings } from './commands'
import { toolRegistry } from './session'

/** Приложения и сайты, которые называют чаще всего. */
const APPS: Record<string, { tool: 'open_app' | 'open_url'; target: string; label: string }> = {
  браузер: { tool: 'open_url', target: 'https://www.google.com', label: 'браузер' },
  хром: { tool: 'open_app', target: 'chrome', label: 'Chrome' },
  гугл: { tool: 'open_url', target: 'https://www.google.com', label: 'Google' },
  ютуб: { tool: 'open_url', target: 'https://www.youtube.com', label: 'YouTube' },
  youtube: { tool: 'open_url', target: 'https://www.youtube.com', label: 'YouTube' },
  блокнот: { tool: 'open_app', target: 'notepad', label: 'блокнот' },
  калькулятор: { tool: 'open_app', target: 'calc', label: 'калькулятор' },
  проводник: { tool: 'open_app', target: 'explorer', label: 'проводник' },
  'диспетчер задач': { tool: 'open_app', target: 'taskmgr', label: 'диспетчер задач' },
  телеграм: { tool: 'open_app', target: 'telegram', label: 'Telegram' },
  telegram: { tool: 'open_app', target: 'telegram', label: 'Telegram' },
  дискорд: { tool: 'open_app', target: 'discord', label: 'Discord' },
  discord: { tool: 'open_app', target: 'discord', label: 'Discord' },
  стим: { tool: 'open_app', target: 'steam', label: 'Steam' },
  steam: { tool: 'open_app', target: 'steam', label: 'Steam' },
  спотифай: { tool: 'open_app', target: 'spotify', label: 'Spotify' },
  spotify: { tool: 'open_app', target: 'spotify', label: 'Spotify' },
  'vs code': { tool: 'open_app', target: 'code', label: 'VS Code' },
  'visual studio code': { tool: 'open_app', target: 'code', label: 'VS Code' },
  код: { tool: 'open_app', target: 'code', label: 'VS Code' },
  почту: { tool: 'open_url', target: 'https://mail.google.com', label: 'почту' },
}

/** Что сказать в ответ. Несколько вариантов — чтобы не звучать автоответчиком. */
const OK = ['Готово.', 'Сделала.', 'Есть.']
const OPENING = ['Открываю.', 'Сейчас открою.', 'Открываю, секунду.']
const pick = (list: string[]) => list[Math.floor(Math.random() * list.length)]!

/** Нижний регистр, «ё» как «е», без знаков по краям. */
export function normalize(text: string): string {
  return text
    .toLowerCase()
    .replace(/ё/g, 'е')
    .replace(/[«»"“”.,!?…:;]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/^(пожалуйста|а|ну|слушай)\s+/, '')
    .replace(/\s+пожалуйста$/, '')
}

/** Что понято из фразы. */
export type Intent =
  | { kind: 'stop' }
  | { kind: 'open'; tool: 'open_app' | 'open_url'; target: string; label: string }
  | { kind: 'media'; action: 'pause' | 'play' | 'next' | 'previous' }
  | { kind: 'volume'; level: number }
  | { kind: 'avatar'; action: string; reply: string }
  | { kind: 'time' }

const AVATAR: [RegExp, string, string][] = [
  [/^(станцуй|потанцуй|танцуй|давай потанцуем)/, 'builtin:dance', 'С удовольствием!'],
  [/^(помаши|поздоровайся)/, 'builtin:wave', 'Привет-привет!'],
  [/^(сядь|присядь|посиди)/, 'builtin:sit', 'Хорошо, посижу.'],
  [/^(встань|вставай)/, 'builtin:stand', 'Встаю.'],
  [/^(поспи|иди спать|отдохни)/, 'builtin:sleep', 'Немного вздремну.'],
  [/^(прогуляйся|погуляй)/, 'builtin:walk', 'Пойду прогуляюсь.'],
]

/** Узнаёт фразу. `null` — не уверены, пусть решает модель. */
export function recognize(raw: string): Intent | null {
  const text = normalize(raw)
  if (!text) return null

  if (/^(стоп|хватит|замолчи|помолчи|тихо|перестань|отмена|отмени)$/.test(text)) return { kind: 'stop' }

  const open = /^(открой|запусти|включи)\s+(.+)$/.exec(text)
  if (open) {
    const app = APPS[open[2]!]
    // Незнакомое имя модели виднее: «включи музыку» — это не приложение «музыку».
    if (app) return { kind: 'open', ...app }
    return null
  }

  if (/^(пауза|поставь на паузу|останови музыку|стоп музыка|выключи музыку)$/.test(text)) return { kind: 'media', action: 'pause' }
  if (/^(продолжи|играй|включи музыку|продолжи музыку|сними с паузы)$/.test(text)) return { kind: 'media', action: 'play' }
  if (/^(следующий трек|следующая песня|дальше|следующий|переключи трек)$/.test(text)) return { kind: 'media', action: 'next' }
  if (/^(предыдущий трек|предыдущая песня|назад|верни трек)$/.test(text)) return { kind: 'media', action: 'previous' }

  if (/^(выключи звук|без звука)$/.test(text)) return { kind: 'volume', level: 0 }
  const volume = /^(?:громкость|звук|сделай звук|поставь громкость)\s*(?:на)?\s*(\d{1,3})\s*(?:%|процент\S*)?$/.exec(text)
  if (volume) return { kind: 'volume', level: Math.min(100, Number(volume[1])) / 100 }

  if (/^(который час|сколько времени|сколько сейчас времени)$/.test(text)) return { kind: 'time' }

  for (const [pattern, action, reply] of AVATAR) {
    if (pattern.test(text)) return { kind: 'avatar', action, reply }
  }
  return null
}

/**
 * Выполняет инструмент через тот же Permission Gate, что и модель.
 *
 * Если действие требует подтверждения или запрещено, быстрый путь уступает
 * модели: там есть и диалог подтверждения, и объяснение отказа.
 */
async function runTool(id: string, input: unknown): Promise<boolean> {
  const tool = toolRegistry().get(id)
  if (!tool) return false
  const decision = evaluate(tool, await gateSettings())
  if (decision.kind !== 'allow') return false
  const started = performance.now()
  try {
    await tool.execute(input, { signal: new AbortController().signal, report: () => undefined, attach: () => undefined })
    void activityRecord({ tool: id, status: 'ok', durationMs: Math.round(performance.now() - started) }).catch(() => undefined)
    return true
  } catch (error) {
    void activityRecord({ tool: id, status: 'error', result: String(error) }).catch(() => undefined)
    return false
  }
}

/**
 * Записывает быстрый ход в чат и историю: модель должна знать, что уже сделано.
 *
 * В историю идёт свершившийся факт, а не то, что сказано вслух. Раньше туда
 * попадало «Открываю, секунду» — и на следующую же реплику, даже «привет»,
 * модель открывала блокнот второй раз: обещание без выполнения она честно
 * принимала за невыполненную просьбу.
 */
function record(said: string, reply: string, done = reply) {
  const chat = useChatStore.getState()
  const history: Message[] = [
    ...chat.history,
    { role: 'user', content: [{ type: 'text', text: said }] },
    { role: 'assistant', content: [{ type: 'text', text: `${done} (выполнено, повторять не нужно)` }] },
  ]
  chat.startTurn(said)
  chat.finishTurn(reply, history)
  useUiStore.getState().flashResult('success')
}

/**
 * Пробует выполнить фразу быстро. Возвращает ответ, который нужно произнести,
 * или `null`, если фраза не узнана и её надо отдать модели.
 */
export async function tryQuick(said: string): Promise<string | null> {
  const intent = recognize(said)
  if (!intent) return null

  switch (intent.kind) {
    case 'stop':
      await voiceStopSpeaking().catch(() => undefined)
      // На «стоп» не отвечают — просто замолкают.
      return ''
    case 'open': {
      const input = intent.tool === 'open_app' ? { app: intent.target } : { url: intent.target }
      if (!(await runTool(intent.tool, input))) return null
      const reply = pick(OPENING)
      record(said, reply, `Открыла ${intent.label}.`)
      return reply
    }
    case 'media':
      try {
        await mediaControl(intent.action)
      } catch {
        return null
      }
      record(said, pick(OK), intent.action === 'pause' ? 'Поставила музыку на паузу.' : intent.action === 'play' ? 'Включила музыку.' : 'Переключила трек.')
      // Музыку не перебивают голосом: результат и так слышно.
      return ''
    case 'volume': {
      if (!(await runTool('set_volume', { level: intent.level }))) return null
      const reply = intent.level === 0 ? 'Звук выключен.' : `Громкость ${Math.round(intent.level * 100)}.`
      record(said, reply)
      return reply
    }
    case 'time': {
      const now = new Date().toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })
      const reply = `Сейчас ${now}.`
      record(said, reply)
      return reply
    }
    case 'avatar':
      await avatarPlay(intent.action).catch(() => undefined)
      record(said, intent.reply)
      return intent.reply
  }
}
