/**
 * Permission Gate — единственный путь к выполнению инструмента (ТЗ §21, §22).
 *
 * Проверка идёт в том же порядке, что описан в ТЗ:
 *   1) выдано ли системное разрешение ОС;
 *   2) разрешил ли пользователь категорию внутри Yuki;
 *   3) нужно ли подтверждение по уровню риска.
 *
 * Порядок не случаен: сначала отсекается то, что физически невозможно, и только
 * потом тревожится пользователь. Спрашивать подтверждение на действие, которое
 * всё равно упрётся в отсутствующее разрешение macOS, — впустую потраченный вопрос.
 */

import type { PermissionCategory, RiskLevel, Tool } from '../agent/types'

/** Политика подтверждений для MEDIUM (ТЗ §22: «согласно настройкам»). */
export type MediumRiskPolicy = 'auto' | 'ask'

export interface PermissionState {
  /** Разрешил ли пользователь категорию внутри Yuki. */
  readonly granted: boolean
  /**
   * Выдано ли разрешение на уровне ОС.
   *
   * На macOS Accessibility и Screen Recording выдаются системой отдельно и без
   * них API молча возвращает пустоту, поэтому статус хранится отдельным полем.
   */
  readonly osGranted: boolean
}

export interface GateSettings {
  readonly permissions: ReadonlyMap<PermissionCategory, PermissionState>
  readonly mediumRiskPolicy: MediumRiskPolicy
}

export type GateDecision =
  | { readonly kind: 'allow' }
  | { readonly kind: 'confirm'; readonly risk: RiskLevel }
  | {
      readonly kind: 'deny'
      readonly reason: 'os_permission_missing' | 'user_permission_missing'
      readonly category: PermissionCategory
    }

/**
 * Решает, что делать с вызовом инструмента.
 *
 * Функция намеренно чистая: она ничего не выполняет и ни о чём не спрашивает —
 * только выносит решение. Это позволяет протестировать всю матрицу ТЗ §21–§22
 * без единого системного вызова.
 */
export function evaluate(tool: Tool, settings: GateSettings): GateDecision {
  for (const category of tool.permissions) {
    const state = settings.permissions.get(category)

    if (!state?.osGranted) {
      return { kind: 'deny', reason: 'os_permission_missing', category }
    }
    if (!state.granted) {
      return { kind: 'deny', reason: 'user_permission_missing', category }
    }
  }

  if (tool.risk === 'high') {
    return { kind: 'confirm', risk: 'high' }
  }
  if (tool.risk === 'medium' && settings.mediumRiskPolicy === 'ask') {
    return { kind: 'confirm', risk: 'medium' }
  }
  return { kind: 'allow' }
}

/** Удобный конструктор состояния разрешений для вызывающего кода и тестов. */
export function permissionMap(
  entries: Partial<Record<PermissionCategory, PermissionState>>,
): ReadonlyMap<PermissionCategory, PermissionState> {
  return new Map(Object.entries(entries) as [PermissionCategory, PermissionState][])
}
