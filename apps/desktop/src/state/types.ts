/** Состояния Orb из ТЗ §13. */
export type OrbState =
  | 'idle'
  | 'listening'
  | 'thinking'
  | 'working'
  | 'speaking'
  | 'success'
  | 'error'
  | 'sleeping'

/** Разделы навигационной рейки (ТЗ §13). */
export type ScreenId =
  | 'orbital'
  | 'chat'
  | 'commands'
  | 'memory'
  | 'activity'
  | 'settings'
  /**
   * Capability Hub. В рейке его нет намеренно: ТЗ §13 задаёт её состав, а ТЗ §17
   * помещает Hub в настройки — оттуда он и открывается.
   */
  | 'capabilities'

/** Статусы задачи (ТЗ §32). */
export type TaskStatus =
  | 'queued'
  | 'running'
  | 'waiting_user'
  | 'completed'
  | 'failed'
  | 'cancelled'

export interface Task {
  id: string
  title: string
  status: TaskStatus
  /** Человекочитаемое описание текущего шага — то, что показывается в UI. */
  currentStep?: string
  /** 0…1. */
  progress: number
}

/**
 * Строка журнала активности (ТЗ §23).
 *
 * Это ровно то, что видит пользователь: безопасный статус вида «Ищу файл…».
 * Внутренняя цепочка рассуждений сюда не попадает — ТЗ §15 запрещает её показ.
 */
export interface ActivityItem {
  id: string
  ts: number
  tool: string
  target?: string
  status: 'ok' | 'error' | 'cancelled' | 'denied'
  durationMs?: number
}

/** Ближайшее событие календаря для нижней полосы Orbital-экрана (ТЗ §13). */
export interface UpcomingEvent {
  title: string
  startsAt: number
}

export interface ConnectionState {
  /** Есть ли доступ к выбранному AI-провайдеру. */
  online: boolean
  /** Метка активного провайдера: «Claude», «Ollama · локально». */
  providerLabel: string
}
