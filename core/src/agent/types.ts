/**
 * Базовые типы Agent Core (ТЗ §5, §21, §22, §32).
 *
 * Главный инвариант ТЗ §5 и §44 — «Yuki никогда не должна заявлять об успешном
 * действии без подтверждения tool» — здесь выражен типом, а не соглашением:
 * у {@link ToolResult} нет варианта «наверное получилось». Либо `ok` с данными
 * от инструмента, либо `error` с причиной. Третьего варианта система не знает.
 */

/** Категории разрешений (ТЗ §21). */
export type PermissionCategory =
  | 'microphone'
  | 'screen_recording'
  | 'accessibility'
  | 'files'
  | 'network'
  | 'shell'
  | 'camera'
  | 'notifications'
  | 'browser'
  | 'external_services'

/**
 * Уровень риска действия (ТЗ §22).
 *
 * `low` — выполняется автоматически;
 * `medium` — по настройке пользователя;
 * `high` — всегда требует подтверждения с показом плана.
 */
export type RiskLevel = 'low' | 'medium' | 'high'

/** Действия, которые ТЗ §22 относит к HIGH безусловно. */
export const ALWAYS_HIGH_RISK = [
  'delete',
  'shutdown',
  'restart',
  'shell',
  'system_change',
  'financial',
  'important_message',
] as const

export type AlwaysHighRisk = (typeof ALWAYS_HIGH_RISK)[number]

/** Результат работы инструмента. */
export type ToolResult<T = unknown> =
  | { readonly ok: true; readonly value: T; readonly durationMs: number }
  | { readonly ok: false; readonly error: ToolError; readonly durationMs: number }

export interface ToolError {
  /** Машинно-читаемая причина — по ней строится восстановление (ТЗ §33). */
  readonly kind:
    | 'not_found'
    | 'permission_denied'
    | 'invalid_argument'
    | 'unsupported'
    | 'not_implemented'
    | 'timeout'
    | 'cancelled'
    | 'platform'
  /** Формулировка для пользователя: реальная причина, а не «что-то пошло не так» (ТЗ §33). */
  readonly message: string
}

export type { ToolSpec } from './protocol'

/** Инструмент в реестре (ТЗ §4 Tool Registry). */
export interface Tool<Input = unknown, Output = unknown> {
  readonly id: string
  readonly name: string
  readonly description: string
  readonly permissions: readonly PermissionCategory[]
  readonly risk: RiskLevel
  /**
   * Идемпотентна ли операция.
   *
   * ТЗ §33 разрешает автоматический повтор только для безопасных и идемпотентных
   * операций, поэтому признак обязателен и по умолчанию считается ложным.
   */
  readonly idempotent: boolean
  /** JSON Schema входа — её же получает модель при выборе инструмента. */
  readonly inputSchema: Record<string, unknown>
  execute(input: Input, ctx: ToolContext): Promise<Output>
}

/** Картинка, которую инструмент отдаёт модели вместе с результатом. */
export interface ToolAttachment {
  readonly mediaType: string
  /** Содержимое в base64. */
  readonly data: string
}

export interface ToolContext {
  /** Отмена задачи пользователем (ТЗ §32). */
  readonly signal: AbortSignal
  /** Сообщает UI безопасный статус вида «Ищу файл…» (ТЗ §15). */
  report(status: string): void
  /**
   * Прикладывает изображение к результату инструмента (ТЗ §6).
   *
   * Отдельный канал, а не возвращаемое значение: снимок экрана весит сотни
   * килобайт, и ему нечего делать ни в журнале активности (ТЗ §23), ни в
   * тексте, который увидит пользователь. Модель получит его блоком, а журнал —
   * только размеры.
   */
  attach(attachment: ToolAttachment): void
}

/** Шаг плана, построенного агентом (ТЗ §5). */
export interface PlanStep {
  readonly id: string
  readonly toolId: string
  readonly input: unknown
  /** Формулировка для пользователя — она показывается в модалке подтверждения (ТЗ §22). */
  readonly summary: string
}

export interface Plan {
  readonly goal: string
  readonly steps: readonly PlanStep[]
  /** Максимальный риск среди шагов: по нему решается, нужно ли подтверждение. */
  readonly risk: RiskLevel
}

const RISK_ORDER: Record<RiskLevel, number> = { low: 0, medium: 1, high: 2 }

/** Наибольший из двух уровней риска. */
export function maxRisk(a: RiskLevel, b: RiskLevel): RiskLevel {
  return RISK_ORDER[a] >= RISK_ORDER[b] ? a : b
}

/** Успешный результат. */
export function ok<T>(value: T, durationMs: number): ToolResult<T> {
  return { ok: true, value, durationMs }
}

/** Неуспешный результат. */
export function fail(
  kind: ToolError['kind'],
  message: string,
  durationMs: number,
): ToolResult<never> {
  return { ok: false, error: { kind, message }, durationMs }
}

/**
 * Можно ли безопасно повторить операцию после ошибки (ТЗ §33, шаг 3).
 *
 * Повтор разрешён только для идемпотентных инструментов и только для ошибок,
 * которые могли быть временными. Отказ в разрешении или неверный аргумент
 * повтором не лечатся — повторять их значит дёргать пользователя впустую.
 */
export function isRetryable(tool: Tool, error: ToolError): boolean {
  if (!tool.idempotent) return false
  return error.kind === 'timeout' || error.kind === 'platform'
}
