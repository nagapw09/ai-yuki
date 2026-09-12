/**
 * Подключение агентного цикла к приложению (ТЗ §5).
 *
 * Здесь сходятся четыре вещи, которые ядро намеренно не знает: как позвать модель
 * (через Rust), какие разрешения выданы (из базы), как спросить подтверждение
 * (модалка) и куда писать журнал (ТЗ §23).
 */

import {
  ToolRegistry,
  evaluate,
  permissionMap,
  runAgent,
  type ChatFn,
  type ChatResponse,
  type GateSettings,
  type Message,
  type PermissionCategory,
  type PermissionState,
  type Tool,
} from '@yuki/core'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

import { activityRecord, memoryContext, permissionsList, settingGet } from '../bridge'
import { useChatStore } from '../state/chatStore'
import { useUiStore } from '../state/store'
import { mcpTools } from '../tools/capabilities'
import { ALL_TOOLS } from '../tools'
import { tryRun } from './commands'
import { speakIfVoice } from './voice'

/** Реестр создаётся один раз: инструменты не меняются в течение сессии. */
const registry = new ToolRegistry().registerAll(ALL_TOOLS)

/**
 * Подтягивает инструменты подключённых MCP-серверов (ТЗ §19).
 *
 * Перед каждым запросом, а не один раз при старте: пользователь мог подключить
 * или выключить сервер прямо в разговоре — именно этого и требует сценарий
 * самораcширения из ТЗ §17, где Yuki добавляет возможность и тут же ей пользуется.
 */
async function syncMcpTools(): Promise<void> {
  registry.replacePrefixed('mcp__', await mcpTools())
}

export function toolRegistry(): ToolRegistry {
  return registry
}

/**
 * Откуда пришла просьба.
 *
 * Локальная и удалённая просьбы проходят один и тот же цикл с одним и тем же
 * Permission Gate — второй путь означал бы вторую реализацию разрешений. Но
 * набор инструментов у них разный, и журнал должен различать, с чего действие
 * началось: `docs/REMOTE-CONTROL.md` §4 — «журнал, не различающий локальное и
 * удалённое, бесполезен ровно тогда, когда нужен».
 */
export interface RemoteOrigin {
  channel: 'telegram'
  /** Куда отвечать. */
  chatId: string
  /** Имя устройства для журнала. */
  device: string
  /** Открыт ли полный набор инструментов (`remote.full_access`). */
  fullAccess: boolean
  /** Сообщить на телефон, что ждём подтверждения у компьютера. */
  notify: (text: string) => void
}

/**
 * Инструменты, доступные с телефона без особого разрешения.
 *
 * `docs/REMOTE-CONTROL.md` §4: по умолчанию — разговор, напоминания, заметки,
 * статус задач; управление файлами и вводом включается явно. Здесь к ним
 * добавлены только те, что ничего не меняют: погода, курсы, сведения о
 * системе, чтение календаря и памяти.
 *
 * Удаление из списка отсутствует намеренно: удалённо человек не видит экрана и
 * не может проверить, что удаляется именно то, что он имел в виду.
 */
const REMOTE_TOOLS: readonly string[] = [
  'create_reminder',
  'list_reminders',
  'save_note',
  'list_notes',
  'list_commands',
  'calendar_events',
  'recall',
  'remember',
  'weather',
  'currency_rates',
  'system_info',
  'notify',
  'avatar_animate',
]

/** Реестр для удалённого хода: тот же набор инструментов, но урезанный. */
function remoteRegistry(origin: RemoteOrigin): ToolRegistry {
  if (origin.fullAccess) return registry

  return new ToolRegistry().registerAll(
    registry.list().filter((tool) => REMOTE_TOOLS.includes(tool.id)),
  )
}

interface ChatDeltaEvent {
  requestId: string
  text: string
}

let requestCounter = 0

/**
 * Обмен с моделью через Rust с потоковой отдачей текста в UI.
 *
 * Подписка на события живёт ровно один запрос: слушатель, переживший свой вызов,
 * начал бы дописывать текст в следующий ход.
 */
function createChat(systemExtra: string | undefined): ChatFn {
  return async ({ messages, tools }) => {
    const requestId = `req-${(requestCounter += 1)}`

    const unlisten = await listen<ChatDeltaEvent>('yuki://chat-delta', (event) => {
      if (event.payload.requestId === requestId) {
        useChatStore.getState().appendDelta(event.payload.text)
      }
    })

    try {
      return await invoke<ChatResponse>('chat_send', {
        args: {
          requestId,
          messages,
          tools,
          systemExtra: systemExtra ?? null,
          providerId: null,
          model: null,
          maxTokens: null,
          temperature: null,
        },
      })
    } finally {
      unlisten()
    }
  }
}

/** Текущие разрешения из базы (ТЗ §21). */
async function loadGateSettings(): Promise<GateSettings> {
  const rows = await permissionsList()
  const entries: Partial<Record<PermissionCategory, PermissionState>> = {}
  for (const row of rows) {
    entries[row.category as PermissionCategory] = {
      granted: row.granted,
      osGranted: row.osGranted,
    }
  }

  const policy = await settingGet('confirm.medium_risk')
  return {
    permissions: permissionMap(entries),
    // По умолчанию средний риск спрашивает: молчаливое выполнение действия,
    // которое ТЗ §22 отнесло к спорным, — не то, чем стоит удивлять пользователя.
    mediumRiskPolicy: policy === 'auto' ? 'auto' : 'ask',
  }
}

/**
 * Собирает добавку к системной инструкции: время, память и настройку роли.
 *
 * Время обязательно и идёт первым: без него «напомни завтра в 10:00» модель
 * посчитать не может — она не знает ни текущего момента, ни часового пояса
 * пользователя, а ошибка здесь тихая и обнаружится только когда напоминание
 * не сработает.
 *
 * Сбой любой части не должен ломать запрос: без памяти Yuki работает хуже,
 * но работает, а без ответа — нет.
 */
async function buildSystemExtra(): Promise<string | undefined> {
  const parts: string[] = []

  const now = new Date()
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone
  parts.push(
    `Сейчас ${now.toLocaleString('ru-RU')} (${zone}), ` +
      `unix-время ${Math.floor(now.getTime() / 1000)}.`,
  )

  const memory = await memoryContext().catch(() => '')
  if (memory) parts.push(memory)

  const persona = await settingGet('persona.extra').catch(() => null)
  if (persona) parts.push(persona)

  return parts.length > 0 ? parts.join('\n\n') : undefined
}

/** Человеческое описание того, что именно произойдёт (ТЗ §22). */
function describePlan(tool: Tool, input: unknown): string {
  if (input && typeof input === 'object' && Object.keys(input).length > 0) {
    const args = Object.entries(input as Record<string, unknown>)
      .map(([key, value]) => `${key}: ${JSON.stringify(value)}`)
      .join('\n')
    return `${tool.name}\n\n${args}`
  }
  return tool.name
}

/**
 * Обрабатывает одну реплику пользователя целиком.
 *
 * Возвращается после того, как ход завершён: и успех, и ошибка уже отражены
 * в состоянии — вызывающему коду ничего доделывать не нужно.
 */
export async function sendMessage(
  text: string,
  origin?: RemoteOrigin,
): Promise<string | null> {
  // Сначала команды (ТЗ §16): записанная последовательность выполняется сразу,
  // без обращения к модели — в этом весь её смысл.
  if (await tryRun(text).catch(() => false)) return null

  const chat = useChatStore.getState()
  const ui = useUiStore.getState()

  chat.startTurn(text)
  ui.setOrbState('thinking')
  ui.setHeadline(null)

  const userMessage: Message = { role: 'user', content: [{ type: 'text', text }] }
  const history = [...chat.history, userMessage]

  let settings: GateSettings
  try {
    settings = await loadGateSettings()
  } catch (error) {
    chat.failTurn(describeError(error))
    ui.flashResult('error')
    return null
  }

  const systemExtra = await buildSystemExtra()
  await syncMcpTools()

  try {
    const outcome = await runAgent(history, {
      chat: createChat(origin ? remoteExtra(systemExtra, origin) : systemExtra),
      registry: origin ? remoteRegistry(origin) : registry,
      decide: (tool) => evaluate(tool, settings),

      confirm: async (tool, input) => {
        // Пока пользователь думает, Orb не должен изображать работу.
        useUiStore.getState().setOrbState('idle')

        // Подтверждение спрашивается у компьютера, а не у телефона
        // (docs/REMOTE-CONTROL.md §4): удалённое подтверждение опасного
        // действия означает, что укравший телефон получил права владельца.
        // Но молчать нельзя — иначе с телефона это выглядит как зависание.
        origin?.notify(
          `Нужно подтверждение на компьютере: ${tool.name}. ` +
            'Пока его нет, действие не выполняется.',
        )
        const approved = await useChatStore.getState().askConfirmation({
          toolId: tool.id,
          toolName: tool.name,
          risk: tool.risk === 'high' ? 'high' : 'medium',
          plan: describePlan(tool, input),
        })
        useUiStore.getState().setOrbState('working')
        return approved ? 'allow' : 'deny'
      },

      onEvent: (event) => {
        const store = useChatStore.getState()
        switch (event.kind) {
          case 'tool_started':
            useUiStore.getState().setOrbState('working')
            store.upsertTool({
              id: event.toolId,
              toolId: event.toolId,
              label: event.summary,
              state: 'running',
            })
            break
          case 'tool_finished':
            store.upsertTool({
              id: event.toolId,
              toolId: event.toolId,
              label: registry.get(event.toolId)?.name ?? event.toolId,
              state: event.ok ? 'ok' : 'error',
              detail: event.detail,
              durationMs: event.durationMs,
            })
            useUiStore.getState().setOrbState('thinking')
            break
          case 'blocked':
            store.upsertTool({
              id: event.toolId,
              toolId: event.toolId,
              label: registry.get(event.toolId)?.name ?? event.toolId,
              state: 'blocked',
              detail: event.reason,
            })
            break
          default:
            break
        }
      },

      log: (entry) => {
        // Журнал не должен ронять ход: ТЗ §23 требует записи, но потеря строки
        // журнала — меньшая беда, чем потеря результата уже выполненной работы.
        void activityRecord(
          origin
            ? { ...entry, target: `${origin.channel} · ${origin.device}${entry.target ? ` · ${entry.target}` : ''}` }
            : entry,
        ).catch(() => undefined)
      },
    })

    chat.finishTurn(outcome.reply, outcome.messages)
    ui.flashResult('success')

    // Удалённую просьбу вслух не читаем: человека у компьютера нет, и говорить
    // в пустую комнату незачем.
    if (!origin) {
      // Озвучивание не должно задерживать возврат: ход уже закрыт, а Orb
      // переключится в SPEAKING сам.
      void speakIfVoice(outcome.reply)
    }

    return outcome.reply
  } catch (error) {
    const message = describeError(error)
    useChatStore.getState().failTurn(message)
    useUiStore.getState().flashResult('error')
    void activityRecord({
      tool: 'agent',
      status: 'error',
      result: message,
      target: origin ? `${origin.channel} · ${origin.device}` : undefined,
    }).catch(() => undefined)

    throw error
  }
}

/**
 * Добавка к системной инструкции для удалённого хода.
 *
 * Без неё модель предлагала бы недоступные действия и объясняла отказ
 * собственными догадками: инструмента в списке нет, а почему — неизвестно.
 */
function remoteExtra(base: string | undefined, origin: RemoteOrigin): string {
  const note =
    `Эта просьба пришла с телефона через ${origin.channel}, человека у компьютера нет. ` +
    (origin.fullAccess
      ? 'Доступны все инструменты, но действия высокого риска ждут подтверждения у компьютера — предупреди об этом, если оно понадобится.'
      : 'Доступна только часть инструментов: напоминания, заметки, память, календарь на чтение, погода, курсы, сведения о системе. Файлы, ввод и управление окнами закрыты. Если просьба требует закрытого — скажи об этом прямо, не изображай выполнение.') +
    ' Отвечай коротко: ответ читают в мессенджере.'

  return base ? `${base}

${note}` : note
}

function describeError(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return 'Не удалось выполнить запрос'
}
