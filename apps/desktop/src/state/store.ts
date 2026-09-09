import { create } from 'zustand'

import type {
  ActivityItem,
  ConnectionState,
  OrbState,
  ScreenId,
  Task,
  UpcomingEvent,
} from './types'

/**
 * Как долго держится терминальное состояние Orb перед возвратом в IDLE.
 *
 * ТЗ §13 называет SUCCESS «короткой позитивной анимацией», а ERROR — «спокойной
 * индикацией»: ошибка должна оставаться на экране дольше, чтобы её успели заметить.
 */
const SUCCESS_HOLD_MS = 1600
const ERROR_HOLD_MS = 2600

interface UiState {
  screen: ScreenId
  orbState: OrbState
  /** Амплитуда звука 0…1 для LISTENING и SPEAKING. */
  audioLevel: number
  /** Реплика под Orb: «Чем займёмся?» в покое, статус задачи в работе. */
  headline: string | null
  tasks: Task[]
  activity: ActivityItem[]
  nextEvent: UpcomingEvent | null
  connection: ConnectionState

  setScreen: (screen: ScreenId) => void
  setOrbState: (state: OrbState) => void
  setAudioLevel: (level: number) => void
  setHeadline: (headline: string | null) => void
  /** Показывает терминальное состояние и сам возвращает Orb в IDLE. */
  flashResult: (kind: 'success' | 'error') => void
  upsertTask: (task: Task) => void
  removeTask: (id: string) => void
  pushActivity: (item: ActivityItem) => void
  setConnection: (connection: ConnectionState) => void
  setNextEvent: (event: UpcomingEvent | null) => void
}

/** Таймер возврата из терминального состояния; храним вне store, чтобы не рендерить его. */
let flashTimer: ReturnType<typeof setTimeout> | undefined

export const useUiStore = create<UiState>((set, get) => ({
  screen: 'orbital',
  orbState: 'idle',
  audioLevel: 0,
  headline: null,
  tasks: [],
  activity: [],
  nextEvent: null,
  connection: { online: false, providerLabel: '' },

  setScreen: (screen) => set({ screen }),

  setOrbState: (orbState) => {
    // Ручная смена состояния отменяет запланированный возврат в IDLE: иначе
    // отложенный таймер погасит уже начавшуюся новую задачу.
    if (flashTimer !== undefined) {
      clearTimeout(flashTimer)
      flashTimer = undefined
    }
    set({ orbState })
  },

  setAudioLevel: (audioLevel) => set({ audioLevel }),

  setHeadline: (headline) => set({ headline }),

  flashResult: (kind) => {
    if (flashTimer !== undefined) clearTimeout(flashTimer)
    set({ orbState: kind })
    flashTimer = setTimeout(
      () => {
        flashTimer = undefined
        // Возвращаемся в IDLE, только если состояние с тех пор не сменилось —
        // иначе затрём уже начавшуюся работу.
        if (get().orbState === kind) set({ orbState: 'idle' })
      },
      kind === 'success' ? SUCCESS_HOLD_MS : ERROR_HOLD_MS,
    )
  },

  upsertTask: (task) =>
    set((s) => {
      const index = s.tasks.findIndex((t) => t.id === task.id)
      if (index === -1) return { tasks: [...s.tasks, task] }
      const tasks = [...s.tasks]
      tasks[index] = task
      return { tasks }
    }),

  removeTask: (id) => set((s) => ({ tasks: s.tasks.filter((t) => t.id !== id) })),

  // Журнал в памяти держим коротким: полная история живёт в SQLite (ТЗ §23).
  pushActivity: (item) => set((s) => ({ activity: [item, ...s.activity].slice(0, 50) })),

  setConnection: (connection) => set({ connection }),

  setNextEvent: (nextEvent) => set({ nextEvent }),
}))

/** Активные задачи — то, что показывает нижняя полоса Orbital-экрана (ТЗ §13). */
export const selectActiveTasks = (s: UiState): Task[] =>
  s.tasks.filter((t) => t.status === 'running' || t.status === 'queued' || t.status === 'waiting_user')
