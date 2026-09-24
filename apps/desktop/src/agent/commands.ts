/**
 * Запуск команд пользователя (ТЗ §16).
 *
 * Команда выполняется без модели: это её главное свойство. «Юки, запусти рабочий
 * режим» открывает четыре приложения за доли секунды и не стоит ни одного токена,
 * потому что решать тут нечего — последовательность уже записана.
 */

import {
  dayKey,
  isDue,
  parseSchedule,
  evaluate,
  type RunOutcome,
  matchCommand,
  permissionMap,
  runCommand,
  type Command,
  type GateSettings,
  type PermissionCategory,
  type PermissionState,
  type Step,
} from '@yuki/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import {
  activityRecord,
  commandList,
  listWindows,
  permissionsList,
  settingGet,
  type CommandRecord,
} from '../bridge'
import { useChatStore } from '../state/chatStore'
import { useUiStore } from '../state/store'
import { toolRegistry } from './session'

/** Переводит запись из базы в модель ядра. */
function toCommand(record: CommandRecord): Command {
  const trigger: Command['trigger'] =
    record.triggerKind === 'phrase'
      ? { kind: 'phrase', phrase: record.phrase ?? '' }
      : record.triggerKind === 'hotkey'
        ? { kind: 'hotkey', shortcut: record.hotkey ?? '' }
        : record.triggerKind === 'startup'
          ? { kind: 'startup' }
          : record.triggerKind === 'schedule'
            ? { kind: 'schedule', schedule: record.schedule ?? '' }
            : record.triggerKind === 'app'
              ? { kind: 'app', app: record.app ?? '' }
              : { kind: 'manual' }

  return {
    id: record.id,
    name: record.name,
    description: record.description,
    trigger,
    enabled: record.enabled,
    steps: record.steps as Step[],
  }
}

async function loadCommands(): Promise<Command[]> {
  const records = await commandList().catch(() => [])
  return records.map(toCommand)
}

/** Разрешения и политика подтверждений — те же, что у действий модели. */
export async function gateSettings(): Promise<GateSettings> {
  const rows = await permissionsList()
  const entries: Partial<Record<PermissionCategory, PermissionState>> = {}
  for (const row of rows) {
    entries[row.category as PermissionCategory] = {
      granted: row.granted,
      osGranted: row.osGranted,
    }
  }

  const policy = await settingGet('confirm.medium_risk').catch(() => null)
  return {
    permissions: permissionMap(entries),
    mediumRiskPolicy: policy === 'auto' ? 'auto' : 'ask',
  }
}

/** Выполняет команду и показывает ход в чате. */
export async function execute(command: Command): Promise<RunOutcome> {
  const chat = useChatStore.getState()
  const ui = useUiStore.getState()

  chat.startTurn(command.name)
  ui.setOrbState('working')

  const settings = await gateSettings()
  const labels = new Map<number, string>()

  const outcome = await runCommand(command, {
    registry: toolRegistry(),
    decide: (tool) => evaluate(tool, settings),

    confirm: async (tool, input) => {
      ui.setOrbState('idle')
      const approved = await useChatStore.getState().askConfirmation({
        toolId: tool.id,
        toolName: tool.name,
        risk: tool.risk === 'high' ? 'high' : 'medium',
        plan:
          input && typeof input === 'object' && Object.keys(input).length > 0
            ? `${tool.name}\n\n${Object.entries(input as Record<string, unknown>)
                .map(([k, v]) => `${k}: ${JSON.stringify(v)}`)
                .join('\n')}`
            : tool.name,
      })
      ui.setOrbState('working')
      return approved
    },

    onEvent: (event) => {
      const store = useChatStore.getState()
      // Название шага приходит только в step_started; дальше события несут
      // индекс, поэтому подпись запоминается здесь, а не тянется в каждое.
      switch (event.kind) {
        case 'step_started':
          labels.set(event.index, event.label)
          store.upsertTool({
            id: `step-${event.index}`,
            toolId: event.label,
            label: event.label,
            state: 'running',
          })
          break
        case 'step_finished':
          store.upsertTool({
            id: `step-${event.index}`,
            toolId: labels.get(event.index) ?? `шаг ${event.index + 1}`,
            label: labels.get(event.index) ?? `шаг ${event.index + 1}`,
            state: event.ok ? 'ok' : 'error',
            detail: event.detail,
          })
          break
        case 'skipped':
          store.upsertTool({
            id: `step-${event.index}`,
            toolId: labels.get(event.index) ?? `шаг ${event.index + 1}`,
            label: 'шаг пропущен',
            state: 'blocked',
            detail: event.reason,
          })
          break
        default:
          break
      }
    },

    log: (entry) => {
      void activityRecord({ ...entry, target: command.name }).catch(() => undefined)
    },
  })

  if (outcome.ok) {
    chat.finishTurn(`Готово: ${command.name} — шагов ${outcome.executed}.`, chat.history)
    ui.flashResult('success')
  } else {
    chat.finishTurn(outcome.error ?? 'Команда не выполнена', chat.history)
    ui.flashResult('error')
  }

  return outcome
}

/**
 * Пробует выполнить реплику как команду.
 *
 * Возвращает `true`, если команда нашлась и была запущена — тогда обращаться
 * к модели не нужно вовсе.
 */
export async function tryRun(text: string): Promise<boolean> {
  const commands = await loadCommands()
  const found = matchCommand(text, commands)
  if (!found) return false

  await execute(found)
  return true
}

/** Запускает команду по идентификатору — для кнопки «выполнить» и хоткея. */
export async function runById(id: string): Promise<RunOutcome> {
  const commands = await loadCommands()
  const found = commands.find((c) => c.id === id)
  if (!found) {
    return { ok: false, executed: 0, variables: {}, error: 'команда не найдена' }
  }
  return execute(found)
}

let unlistenHotkeys: UnlistenFn | null = null

/**
 * Подписывается на сочетания команд и выполняет то, что помечено запуском
 * при старте (ТЗ §16: triggers).
 */
export async function initTriggers(): Promise<void> {
  if (!unlistenHotkeys) {
    unlistenHotkeys = await listen<{ commandId: string }>(
      'yuki://command-trigger',
      (event) => {
        // Окно могло быть свёрнуто: показываем чат, чтобы ход выполнения
        // был виден, а не происходил втайне.
        useUiStore.getState().setScreen('chat')
        void runById(event.payload.commandId)
      },
    )
  }

  const commands = await loadCommands()
  for (const command of commands) {
    if (command.enabled && command.trigger.kind === 'startup') {
      await execute(command).catch(() => undefined)
    }
  }

  startWatchers()
}

// ── Расписание и запуск программ ────────────────────────────────────────────
//
// Оба триггера проверяются опросом, а не событиями ОС: раз в 15 секунд для
// времени и раз в 5 секунд для окон — это копейки по сравнению с тем, что уже
// делает голосовой цикл, зато без хуков в систему.

const SCHEDULE_TICK = 15_000
const APP_TICK = 5_000
const LAST_RUN_KEY = 'yuki-command-schedule'

let watching = false
/** Программы, окна которых были на прошлом шаге: запуск — это появление. */
let seenApps: Set<string> | null = null

function lastRuns(): Record<string, string> {
  try {
    return JSON.parse(localStorage.getItem(LAST_RUN_KEY) ?? '{}') as Record<string, string>
  } catch {
    return {}
  }
}

function markRun(id: string, day: string): void {
  try {
    localStorage.setItem(LAST_RUN_KEY, JSON.stringify({ ...lastRuns(), [id]: day }))
  } catch {
    // Без хранилища команда может повториться после перезапуска — не страшно.
  }
}

async function scheduleTick(): Promise<void> {
  const now = new Date()
  const runs = lastRuns()
  for (const command of await loadCommands()) {
    if (!command.enabled || command.trigger.kind !== 'schedule') continue
    const schedule = parseSchedule(command.trigger.schedule)
    if (!schedule || !isDue(schedule, now, runs[command.id] ?? null)) continue
    // Отмечаем до выполнения: долгая команда не должна стартовать второй раз
    // на следующем шаге опроса.
    markRun(command.id, dayKey(now))
    await execute(command).catch(() => undefined)
  }
}

const normalizeApp = (name: string) => name.toLowerCase().replace(/\.exe$/, '').trim()

async function appTick(): Promise<void> {
  const commands = (await loadCommands()).filter(
    (c) => c.enabled && c.trigger.kind === 'app' && c.trigger.app.trim(),
  )
  if (!commands.length) {
    seenApps = null
    return
  }
  const windows = await listWindows().catch(() => null)
  if (!windows) return
  const now = new Set(windows.map((w) => normalizeApp(w.appName)))
  const before = seenApps
  seenApps = now
  // Первый опрос только запоминает, что уже открыто: программы, запущенные
  // до Yuki, не должны разом запускать свои команды.
  if (!before) return
  for (const command of commands) {
    if (command.trigger.kind !== 'app') continue
    const app = normalizeApp(command.trigger.app)
    const matches = (name: string) => name === app || name.includes(app)
    const appeared = [...now].some(matches) && ![...before].some(matches)
    if (appeared) await execute(command).catch(() => undefined)
  }
}

function startWatchers(): void {
  if (watching) return
  watching = true
  setInterval(() => void scheduleTick(), SCHEDULE_TICK)
  setInterval(() => void appTick(), APP_TICK)
  void scheduleTick()
  void appTick()
}
