/**
 * Типизированный мост к Rust-слою.
 *
 * Единственное место, где встречается `invoke`. Всё остальное приложение работает
 * с функциями отсюда, поэтому переименование команды правится в одном файле, а не
 * ловится в рантайме по строковому литералу.
 *
 * Имена аргументов совпадают с параметрами `#[tauri::command]`: Tauri сам
 * конвертирует camelCase из JS в snake_case Rust.
 */

import { invoke } from '@tauri-apps/api/core'

// ── Типы, зеркалящие yuki-system ────────────────────────────────────────────────

export interface AppInfo {
  pid: number
  name: string
  path: string | null
}

export interface Rect {
  x: number
  y: number
  width: number
  height: number
}

export interface WindowInfo {
  id: number
  title: string
  appName: string
  pid: number
  bounds: Rect
  isFocused: boolean
  isMinimized: boolean
}

export interface SystemInfo {
  platform: string
  osVersion: string
  arch: string
  hostname: string
  cpuCount: number
  totalMemoryBytes: number
  availableMemoryBytes: number
}

export interface FileEntry {
  path: string
  name: string
  isDir: boolean
  sizeBytes: number
  modifiedAt: number | null
  extension: string | null
}

export type FileSort = 'name_asc' | 'modified_desc' | 'size_desc'

export interface FileQuery {
  root: string
  nameContains?: string | null
  extensions: string[]
  maxDepth?: number | null
  sort: FileSort
  limit: number
  includeHidden: boolean
}

export type Modifier = 'ctrl' | 'alt' | 'shift' | 'meta'
export type MouseButton = 'left' | 'right' | 'middle'

export interface ScreenCapture {
  width: number
  height: number
  pngBase64: string
  displayIndex: number
}

export interface AccessibilityNode {
  role: string
  name: string | null
  value: string | null
  bounds: Rect | null
  enabled: boolean
  focused: boolean
  actions: string[]
  children: AccessibilityNode[]
}

export interface PermissionStatus {
  category: string
  granted: boolean
  osGranted: boolean
}

export interface ActivityEntry {
  id: string
  ts: number
  tool: string
  target: string | null
  status: 'ok' | 'error' | 'cancelled' | 'denied'
  result: string | null
  durationMs: number | null
}

// ── Приложения и окна (ТЗ §6, §30) ──────────────────────────────────────────────

export const openApp = (app: string) => invoke<AppInfo>('open_app', { app })
export const closeApp = (app: string, force = false) => invoke<void>('close_app', { app, force })
export const listApps = () => invoke<AppInfo[]>('list_apps')
export const listWindows = () => invoke<WindowInfo[]>('list_windows')
export const focusWindow = (windowId: number) => invoke<void>('focus_window', { windowId })
export const activeWindow = () => invoke<WindowInfo | null>('active_window')
export const systemInfo = () => invoke<SystemInfo>('system_info')
export const getVolume = () => invoke<number>('get_volume')
export const setVolume = (level: number) => invoke<void>('set_volume', { level })

// ── Файлы (ТЗ §8) ───────────────────────────────────────────────────────────────

/** Значения по умолчанию, чтобы вызывающий код задавал только то, что важно. */
export function fileQuery(root: string, overrides: Partial<FileQuery> = {}): FileQuery {
  return {
    root,
    nameContains: null,
    extensions: [],
    maxDepth: 8,
    sort: 'modified_desc',
    limit: 100,
    includeHidden: false,
    ...overrides,
  }
}

export const fileSearch = (query: FileQuery) => invoke<FileEntry[]>('file_search', { query })
export const fileReadText = (path: string) => invoke<string>('file_read_text', { path })
export const fileWriteText = (path: string, contents: string) =>
  invoke<void>('file_write_text', { path, contents })
export const fileMove = (from: string, to: string) => invoke<void>('file_move', { from, to })
export const fileCopy = (from: string, to: string) => invoke<void>('file_copy', { from, to })
/** Удаление — HIGH risk по ТЗ §22: вызывать только после подтверждения пользователя. */
export const fileDelete = (path: string, toTrash = true) =>
  invoke<void>('file_delete', { path, toTrash })
export const fileStat = (path: string) => invoke<FileEntry>('file_stat', { path })
export const fileOpen = (path: string) => invoke<void>('file_open', { path })

// ── Ввод (ТЗ §6) ────────────────────────────────────────────────────────────────

export const typeText = (text: string) => invoke<void>('type_text', { text })
export const pressKey = (key: string, modifiers: Modifier[] = []) =>
  invoke<void>('press_key', { key, modifiers })
export const mouseMove = (x: number, y: number) => invoke<void>('mouse_move', { x, y })
export const mouseClick = (button: MouseButton = 'left') => invoke<void>('mouse_click', { button })
export const mouseScroll = (dx: number, dy: number) => invoke<void>('mouse_scroll', { dx, dy })
export const cursorPosition = () => invoke<[number, number]>('cursor_position')

// ── Буфер обмена (ТЗ §16, §24) ──────────────────────────────────────────────────

export const clipboardRead = () => invoke<string | null>('clipboard_read')
export const clipboardWrite = (text: string) => invoke<void>('clipboard_write', { text })

// ── Экран (ТЗ §6) ───────────────────────────────────────────────────────────────

export const screenCapture = (displayIndex?: number) =>
  invoke<ScreenCapture>('screen_capture', { displayIndex: displayIndex ?? null })
export const screenCaptureWindow = (windowId: number) =>
  invoke<ScreenCapture>('screen_capture_window', { windowId })
export const accessibilityTree = (windowId?: number) =>
  invoke<AccessibilityNode>('accessibility_tree', { windowId: windowId ?? null })
export const displayCount = () => invoke<number>('display_count')

// ── Секреты (ТЗ §29) ────────────────────────────────────────────────────────────

/**
 * Записывает секрет в системное хранилище.
 *
 * Обратной функции нет намеренно: значение ключа не должно попадать во фронтенд.
 * Проверить наличие можно через {@link secretExists}.
 */
export const secretSet = (secretRef: string, value: string) =>
  invoke<void>('secret_set', { secretRef, value })
export const secretDelete = (secretRef: string) => invoke<void>('secret_delete', { secretRef })
export const secretExists = (secretRef: string) => invoke<boolean>('secret_exists', { secretRef })

// ── Разрешения, настройки, журнал (ТЗ §21, §23, §31) ───────────────────────────

export const permissionsList = () => invoke<PermissionStatus[]>('permissions_list')
export const permissionSet = (category: string, granted: boolean) =>
  invoke<void>('permission_set', { category, granted })

export const settingGet = (key: string) => invoke<string | null>('setting_get', { key })
export const settingSet = (key: string, value: string) => invoke<void>('setting_set', { key, value })

export const activityLog = (limit = 100) => invoke<ActivityEntry[]>('activity_log', { limit })
export const activityRecord = (entry: {
  tool: string
  target?: string
  status: 'ok' | 'error' | 'cancelled' | 'denied'
  result?: string
  durationMs?: number
}) =>
  invoke<void>('activity_record', {
    tool: entry.tool,
    target: entry.target ?? null,
    status: entry.status,
    result: entry.result ?? null,
    durationMs: entry.durationMs ?? null,
  })

/**
 * Работаем ли внутри Tauri.
 *
 * В браузере при `vite dev` без Tauri команды недоступны — экран должен
 * показывать разметку, а не падать на первом же `invoke`.
 */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

// ── AI-провайдеры (ТЗ §4) ───────────────────────────────────────────────────────

export interface ProviderRecord {
  id: string
  kind: string
  label: string
  baseUrl: string
  defaultModel: string
  isDefault: boolean
  enabled: boolean
  /** Нужен ли ключ: локальные серверы работают без него. */
  requiresKey: boolean
  /** Задан ли ключ. Значение ключа наружу не отдаётся никогда. */
  hasKey: boolean
}

export const providerList = () => invoke<ProviderRecord[]>('provider_list')

export const providerSave = (
  id: string,
  patch: { baseUrl?: string; defaultModel?: string; enabled?: boolean },
) =>
  invoke<void>('provider_save', {
    id,
    baseUrl: patch.baseUrl ?? null,
    defaultModel: patch.defaultModel ?? null,
    enabled: patch.enabled ?? null,
  })

export const providerSetKey = (id: string, key: string) =>
  invoke<void>('provider_set_key', { id, key })
export const providerClearKey = (id: string) => invoke<void>('provider_clear_key', { id })
export const providerSetDefault = (id: string) => invoke<void>('provider_set_default', { id })
/** Test Connection: проверяет ключ и возвращает список моделей (ТЗ §17, §19). */
export const providerTest = (id: string) => invoke<string[]>('provider_test', { id })

/** Где выдать разрешение вручную; null — на этой ОС выдавать нечего (ТЗ §21). */
export const permissionHint = (category: string) =>
  invoke<string | null>('permission_hint', { category })
