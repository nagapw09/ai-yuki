/**
 * Сама запоминает (ТЗ §9).
 *
 * Инструмент «Запомнить» у модели есть, но посреди просьбы «открой браузер» ей
 * не до памяти: за все разговоры он не был вызван ни разу. Поэтому память
 * пополняется отдельно — когда разговор затих, один фоновый запрос читает
 * новые реплики и сверяет их с тем, что уже известно: новый факт добавить,
 * изменившийся обновить (тот же ключ), устаревший забыть. Плюс одна строка
 * эпизода — «чем занимались», живёт месяц.
 *
 * Запоминается только то, что ТЗ разрешает: имя, язык, предпочтения, рабочие
 * сценарии, любимые приложения, люди, о которых часто просят. Выключается на
 * экране «Память».
 */

import { invoke } from '@tauri-apps/api/core'
import type { ChatResponse } from '@yuki/core'

import { memoryDelete, memoryList, memorySave, settingGet } from '../bridge'
import type { ChatEntry } from '../state/chatStore'
import { useChatStore } from '../state/chatStore'

/** Разговор считается затихшим после такой паузы. */
const QUIET_MS = 3 * 60_000
const MARK_KEY = 'yuki-memorized'
const EPISODE_TTL = 30 * 24 * 3600
/** Меньше реплик пользователя — не о чем думать. */
const MIN_USER_TURNS = 2
const MAX_ENTRIES = 30

export interface Digest {
  readonly facts: { key: string; content: string }[]
  readonly forget: string[]
  readonly episode: string | null
}

/** Новые реплики после отметки; если отметка уже вытеснена — последние. */
export function pendingEntries(entries: readonly ChatEntry[], mark: string | null): ChatEntry[] {
  const at = mark ? entries.findIndex((e) => e.id === mark) : -1
  const fresh = at >= 0 ? entries.slice(at + 1) : entries
  return fresh.filter((e) => e.text.trim()).slice(-MAX_ENTRIES)
}

/** Ответ модели → что сохранить. Всё непонятное молча отбрасывается. */
export function parseDigest(text: string): Digest | null {
  const start = text.indexOf('{')
  const end = text.lastIndexOf('}')
  if (start < 0 || end <= start) return null
  let raw: unknown
  try {
    raw = JSON.parse(text.slice(start, end + 1))
  } catch {
    return null
  }
  if (!raw || typeof raw !== 'object') return null
  const r = raw as Record<string, unknown>
  const facts = Array.isArray(r.facts)
    ? r.facts
        .filter(
          (f): f is { key: string; content: string } =>
            !!f && typeof f.key === 'string' && typeof f.content === 'string' && !!f.key.trim() && !!f.content.trim(),
        )
        .map((f) => ({ key: f.key.trim().toLowerCase().slice(0, 60), content: f.content.trim().slice(0, 300) }))
    : []
  const forget = Array.isArray(r.forget)
    ? r.forget.filter((k): k is string => typeof k === 'string' && !!k.trim()).map((k) => k.trim().toLowerCase())
    : []
  const episode = typeof r.episode === 'string' && r.episode.trim() ? r.episode.trim().slice(0, 300) : null
  return { facts, forget, episode }
}

export function buildPrompt(entries: readonly ChatEntry[], known: readonly { key: string; content: string }[], today: string): string {
  const talk = entries
    .map((e) => `${e.role === 'user' ? 'Пользователь' : 'Yuki'}: ${e.text.replace(/\s+/g, ' ').slice(0, 400)}`)
    .join('\n')
  const facts = known.length ? known.map((f) => `- ${f.key}: ${f.content}`).join('\n') : '(пока ничего)'
  return `Служебная задача, не разговор. Ответь только JSON.

Что ты уже помнишь о пользователе:
${facts}

Новый кусок разговора (${today}):
${talk}

Выбери из разговора то, что стоит помнить о пользователе ДОЛГО: как его зовут и как
обращаться, язык, предпочтения (любимый браузер, музыка, громкость), рабочие
сценарии и любимые приложения, люди, о которых он часто просит (только имя и
кем приходится), важные для него планы. Не запоминай: пароли, ключи, номера
карт, здоровье, разовые просьбы («открой блокнот»), содержание чужих переписок,
и то, что пользователь просил не запоминать.

Ключ — короткое имя факта по-русски («имя», «любимый браузер»). Если факт уже
есть и изменился — верни его с тем же ключом. Если он оказался неверным —
положи ключ в forget. Нечего запомнить — пустые массивы.

episode — одна короткая строка о том, чем занимались, без подробностей
(«настраивали звук, писали Сергею привет»); null, если ничего существенного.

{"facts":[{"key":"…","content":"…"}],"forget":["…"],"episode":"…"}`
}

let timer: ReturnType<typeof setTimeout> | null = null
let running = false

/** Вызывается после каждого хода: через три минуты тишины разговор разбирается. */
export function scheduleMemorize(isBusy: () => boolean): void {
  if (timer) clearTimeout(timer)
  timer = setTimeout(() => {
    timer = null
    if (isBusy()) {
      scheduleMemorize(isBusy)
      return
    }
    void memorize().catch(() => undefined)
  }, QUIET_MS)
}

export async function memorize(): Promise<Digest | null> {
  if (running) return null
  if ((await settingGet('memory.auto').catch(() => null)) === 'off') return null
  running = true
  try {
    const entries = useChatStore.getState().entries
    let mark: string | null = null
    try {
      mark = localStorage.getItem(MARK_KEY)
    } catch {
      /* без отметки разберём последние реплики ещё раз — ключи не дадут дублей */
    }
    const fresh = pendingEntries(entries, mark)
    if (fresh.filter((e) => e.role === 'user').length < MIN_USER_TURNS) return null

    const records = await memoryList('long_term')
    const known = records.filter((r) => r.key).map((r) => ({ key: r.key!, content: r.content }))
    const today = new Date().toLocaleDateString('ru-RU')

    const response = await invoke<ChatResponse>('chat_send', {
      args: {
        requestId: `memorize-${Date.now().toString(36)}`,
        messages: [{ role: 'user', content: [{ type: 'text', text: buildPrompt(fresh, known, today) }] }],
        tools: [],
        systemExtra: 'Сейчас ты не разговариваешь, а разбираешь разговор для памяти. Отвечай только JSON без пояснений.',
        providerId: null,
        model: null,
        maxTokens: null,
        temperature: null,
      },
    })
    const text = response.content.map((b) => (b.type === 'text' ? b.text : '')).join('')
    const digest = parseDigest(text)
    if (!digest) return null

    for (const fact of digest.facts) {
      await memorySave({ kind: 'long_term', key: fact.key, content: fact.content, source: 'разговор' })
    }
    for (const key of digest.forget) {
      const record = records.find((r) => r.key?.toLowerCase() === key)
      if (record) await memoryDelete(record.id)
    }
    if (digest.episode) {
      await memorySave({
        kind: 'episodic',
        content: `${today}: ${digest.episode}`,
        source: 'разговор',
        ttlSeconds: EPISODE_TTL,
      })
    }

    const last = fresh.at(-1)
    try {
      if (last) localStorage.setItem(MARK_KEY, last.id)
    } catch {
      /* см. выше */
    }
    return digest
  } finally {
    running = false
  }
}
