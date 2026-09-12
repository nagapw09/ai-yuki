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
import { emit } from '@tauri-apps/api/event'

import type { OrbState } from '../state/types'

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
/** Открывает ссылку в браузере по умолчанию; только http и https (ТЗ §7). */
export const openUrl = (url: string) => invoke<void>('open_url', { url })

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

/** Прямоугольник в координатах монитора. */
export interface CaptureRegion {
  x: number
  y: number
  width: number
  height: number
}

export const screenCapture = (options?: {
  displayIndex?: number
  region?: CaptureRegion
  /** Ограничение ширины; 0 — без ограничения. */
  maxWidth?: number
}) =>
  invoke<ScreenCapture>('screen_capture', {
    displayIndex: options?.displayIndex ?? null,
    region: options?.region ?? null,
    maxWidth: options?.maxWidth ?? null,
  })
/** Строка текста, найденная на экране (ТЗ §6). */
export interface TextLine {
  text: string
  /** Прямоугольник в координатах снятого кадра. */
  rect: CaptureRegion
}

/** Распознаёт текст на экране или в его части (ТЗ §6). */
export const screenReadText = (options?: {
  displayIndex?: number
  region?: CaptureRegion
}) =>
  invoke<TextLine[]>('screen_read_text', {
    displayIndex: options?.displayIndex ?? null,
    region: options?.region ?? null,
  })

export const screenCaptureWindow = (windowId: number) =>
  invoke<ScreenCapture>('screen_capture_window', { windowId })
export const accessibilityTree = (windowId?: number) =>
  invoke<AccessibilityNode>('accessibility_tree', { windowId: windowId ?? null })
/** Дерево интерфейса в компактном текстовом виде (ТЗ §6). */
export const accessibilityText = (windowId?: number) =>
  invoke<string>('accessibility_text', { windowId: windowId ?? null })
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

// ── Приватность: Local Only (ТЗ §29) ───────────────────────────────

export interface PrivacyStatus {
  localOnly: boolean
  /** Провайдер по умолчанию и то, локален ли он. */
  providerLabel: string | null
  providerLocal: boolean
  /** Адрес распознавания речи и то, локален ли он. */
  sttUrl: string | null
  sttLocal: boolean
  /** Что мешает режиму работать прямо сейчас. */
  blockers: string[]
}

export const privacyStatus = () => invoke<PrivacyStatus>('privacy_status')
export const privacySetLocalOnly = (enabled: boolean) =>
  invoke<PrivacyStatus>('privacy_set_local_only', { enabled })

// ── Плагины (ТЗ §18, §20) ───────────────────────────────────────

/** Что будет установлено — показывается до запуска чужого кода. */
export interface PluginReview {
  id: string
  name: string
  version: string
  description: string
  permissions: string[]
  tools: string[]
  /** Командная строка целиком. */
  commandLine: string
  location: string
  /** Непустой список означает отказ в установке. */
  problems: string[]
}

export interface PluginRecord {
  id: string
  name: string
  version: string
  /** `local` · `git` · `dev_folder` · `generated` */
  origin: string
  location: string
  permissions: string[]
  tools: string[]
  enabled: boolean
  installedAt: number
}


// ── Календари (ТЗ §25) ────────────────────────────────────────

export interface CalendarAccount {
  /** `google` · `microsoft` */
  provider: string
  label: string
  /** Где завести приложение и взять client_id. */
  consoleUrl: string
  clientId: string
  connected: boolean
  connectedAt: number | null
}

export interface CalendarEvent {
  id: string
  title: string
  /** RFC 3339 либо ГГГГ-ММ-ДД для события на весь день. */
  start: string
  end: string
  allDay: boolean
  location: string | null
  description: string | null
  link: string | null
}


// ── Мастер первого запуска и требования (docs/GAPS.md §1, §4) ──────────────────────────────

export interface PermissionStep {
  category: string
  /** Согласие внутри Yuki. */
  userGranted: boolean
  /** Разрешение на уровне ОС. */
  osGranted: boolean
  hint: string | null
  canOpenSettings: boolean
  canRequest: boolean
  required: boolean
}

export interface Requirement {
  key: string
  label: string
  required: string
  actual: string
  ok: boolean
}

export interface RequirementsReport {
  ok: boolean
  items: Requirement[]
  avatarOk: boolean
  localOnlyOk: boolean
}

export interface OnboardingStatus {
  completed: boolean
  platform: string
  providerReady: boolean
  providerLabel: string | null
  permissions: PermissionStep[]
  requirements: RequirementsReport
}


// ── Трей, фон и автозапуск (docs/GAPS.md §3) ──────────────────────────────────

export interface BackgroundStatus {
  /** Прятать окно в трей вместо выхода. */
  closeToTray: boolean
  /** Держать главное окно поверх остальных. */
  alwaysOnTop: boolean
  /** Запускаться вместе с системой. */
  autostart: boolean
}

/** Событие перехода на экран — его шлёт меню трея. */
export const NAVIGATE_EVENT = 'yuki://navigate'


// ── Обновления (docs/GAPS.md §2) ──────────────────────────────────────────

export interface UpdateStatus {
  currentVersion: string
  /** Настроен ли канал: есть ли ключ подписи и адрес. */
  configured: boolean
  /** Почему канал не работает. */
  reason: string | null
}

export interface AvailableUpdate {
  version: string
  currentVersion: string
  notes: string | null
  publishedAt: string | null
}

export const updateStatus = () => invoke<UpdateStatus>('update_status')

/** `null` — обновлений нет; это ответ, а не ошибка. */
export const updateCheck = () => invoke<AvailableUpdate | null>('update_check')

/** Скачивает, ставит и перезапускает приложение. */
export const updateInstall = () => invoke<void>('update_install')

export const backgroundStatus = () => invoke<BackgroundStatus>('background_status')
export const backgroundSetCloseToTray = (enabled: boolean) =>
  invoke<void>('background_set_close_to_tray', { enabled })
export const backgroundSetAlwaysOnTop = (enabled: boolean) =>
  invoke<void>('background_set_always_on_top', { enabled })
export const backgroundSetAutostart = (enabled: boolean) =>
  invoke<void>('background_set_autostart', { enabled })

/** Показывает состояние Yuki в трее (docs/GAPS.md §3). */
export const traySetState = (state: OrbState) => invoke<void>('tray_set_state', { state })


// ── Заметки (docs/GAPS.md §5) ──────────────────────────────────

export interface Note {
  id: string
  title: string
  body: string
  /** Закреплённые идут первыми. */
  pinned: boolean
  createdAt: number
  updatedAt: number
}

export const noteList = (query?: string) =>
  invoke<Note[]>('note_list', { query: query ?? null })

export const noteSave = (note: {
  id?: string
  title?: string
  body: string
  pinned?: boolean
}) =>
  invoke<Note>('note_save', {
    id: note.id ?? null,
    title: note.title ?? null,
    body: note.body,
    pinned: note.pinned ?? null,
  })

export const noteDelete = (id: string) => invoke<void>('note_delete', { id })

// ── Погода и курсы (docs/GAPS.md §6) ──────────────────────────────────

export interface DayForecast {
  date: string
  min: number
  max: number
  description: string
}

export interface Weather {
  place: string
  temperature: number
  /** Как ощущается — по ней решают, что надеть. */
  feelsLike: number
  description: string
  windSpeed: number
  forecast: DayForecast[]
}

export interface Rate {
  code: string
  value: number
}

export interface Rates {
  base: string
  date: string
  rates: Rate[]
}

export const weatherGet = (city: string) => invoke<Weather>('weather_get', { city })

export const ratesGet = (base?: string, symbols?: string[]) =>
  invoke<Rates>('rates_get', { base: base ?? null, symbols: symbols ?? null })

// ── Роль и тон (docs/GAPS.md §7) ──────────────────────────────────

export interface Persona {
  /** `assistant` · `coach` · `editor` · `developer` · `custom` */
  role: string
  custom: string
  /** 0 — на «ты», 1 — нейтрально, 2 — на «вы». */
  formality: number
  /** 0 — кратко, 1 — обычно, 2 — подробно. */
  verbosity: number
  address: string
}

export const personaGet = () => invoke<Persona>('persona_get')

/** Возвращает собранную добавку к системной инструкции. */
export const personaSet = (persona: Persona) => invoke<string>('persona_set', { persona })


// ── Перенос данных и диагностика (docs/GAPS.md §11, §14) ──────────────────────────────

export interface TableCount {
  table: string
  rows: number
}

export interface BackupSummary {
  version: number
  counts: TableCount[]
  /** Что придётся ввести заново: секреты в копию не входят. */
  secretsToReenter: string[]
}

export const backupExport = (path: string) =>
  invoke<BackupSummary>('backup_export', { path })

/** Читает файл и рассказывает, что в нём, ничего не меняя. */
export const backupPreview = (path: string) =>
  invoke<BackupSummary>('backup_preview', { path })

export const backupImport = (path: string, mode: 'merge' | 'replace') =>
  invoke<BackupSummary>('backup_import', { path, mode })

/** Отчёт о состоянии в Markdown. Никуда не отправляется. */
export const diagnosticsReport = () => invoke<string>('diagnostics_report')

export const diagnosticsSave = (path: string) =>
  invoke<string>('diagnostics_save', { path })


// ── Слово пробуждения (ТЗ §37) ──────────────────────────────────

export interface WakeStatus {
  /** Записано ли обращение. */
  enrolled: boolean
  /** Сколько образцов нужно всего. */
  needed: number
  /** Сколько уже записано в текущем заходе. */
  recorded: number
  threshold: number | null
}

export const wakeStatus = () => invoke<WakeStatus>('wake_status')

/** Пишет один образец: две секунды с микрофона. */
export const wakeEnrollRecord = () => invoke<WakeStatus>('wake_enroll_record')

export const wakeEnrollFinish = () => invoke<WakeStatus>('wake_enroll_finish')
export const wakeForget = () => invoke<WakeStatus>('wake_forget')

export const onboardingStatus = () => invoke<OnboardingStatus>('onboarding_status')
export const onboardingCompleted = () => invoke<boolean>('onboarding_completed')
export const onboardingFinish = () => invoke<void>('onboarding_finish')
export const onboardingReset = () => invoke<void>('onboarding_reset')

/** Открывает панель системных настроек для категории разрешений. */
export const permissionOpenSettings = (category: string) =>
  invoke<void>('permission_open_settings', { category })

/** Просит ОС показать диалог выдачи, где он есть. */
export const permissionRequestOs = (category: string) =>
  invoke<boolean>('permission_request_os', { category })

export const systemRequirements = () => invoke<RequirementsReport>('system_requirements')

export const calendarAccounts = () => invoke<CalendarAccount[]>('calendar_accounts')

// ── Аватар (ТЗ §12) ────────────────────────────────────────────

export interface AvatarStatus {
  enabled: boolean
  /** Путь к модели; пустая строка — модель не выбрана. */
  model: string
  /** Существует ли файл модели прямо сейчас. */
  modelPresent: boolean
  clickThrough: boolean
  alwaysOnTop: boolean
  open: boolean
  /** Что в кадре: `portrait` — голова и торс, `full` — во весь рост. */
  pose: AvatarPose
  /** `taskbar` прижимает окно к панели задач, `free` оставляет где поставили. */
  anchor: AvatarAnchor
  /** Папка с файлами анимаций; пустая строка — папка не выбрана. */
  animations: string
}

/** Найденный файл анимации. */
export interface AnimationClip {
  /** Имя без расширения — под ним анимацию просят проиграть. */
  name: string
}

export type AvatarPose = 'portrait' | 'full'
export type AvatarAnchor = 'free' | 'taskbar'

/** Событие смены кадра: окно аватара пересчитывает по нему камеру. */
export const AVATAR_POSE_EVENT = 'yuki://avatar-pose'

/** Событие «проиграй анимацию»; пустое имя означает возврат в покой. */
export const AVATAR_PLAY_EVENT = 'yuki://avatar-play'

/** Состояние, которое главное окно транслирует аватару. */
export interface AvatarSignal {
  state: OrbState
  /** Громкость 0…1, когда она известна. */
  audioLevel: number
}

/** Имя события состояния аватара. */
export const AVATAR_EVENT = 'yuki://avatar-state'

/** Профиль персонажа: облик целиком одним набором. */
export interface Profile {
  id: string
  name: string
  /** Применён ли он прямо сейчас. */
  active: boolean
  /** Выбрана ли в нём модель — по этому видно, полон ли облик. */
  hasModel: boolean
  updatedAt: number
}

/** Устройство, подключённое к удалённому каналу (ТЗ §28). */
export interface RemoteDevice {
  id: string
  name: string
  createdAt: number
  lastSeen: number | null
}

/** Состояние канала Telegram. */
export interface TelegramStatus {
  enabled: boolean
  hasToken: boolean
  running: boolean
  /** При Local Only канал работать не может (docs/REMOTE-CONTROL.md §5). */
  localOnly: boolean
  devices: RemoteDevice[]
  pairingCode: string | null
  pairingSecondsLeft: number | null
}

/** Сообщение из сопряжённого чата. */
export const TELEGRAM_MESSAGE_EVENT = 'yuki://telegram-message'

/** Незнакомый чат назвал код сопряжения — подтверждать на этой машине. */
export const TELEGRAM_PAIRING_EVENT = 'yuki://telegram-pairing'

export const telegramStatus = () => invoke<TelegramStatus>('telegram_status')
export const telegramSetToken = (token: string) =>
  invoke<string>('telegram_set_token', { token })
export const telegramClearToken = () => invoke<void>('telegram_clear_token')
export const telegramPair = () => invoke<string>('telegram_pair')
export const telegramApprove = (chatId: string, name: string) =>
  invoke<RemoteDevice[]>('telegram_approve', { chatId, name })
export const telegramRevoke = (id: string) => invoke<RemoteDevice[]>('telegram_revoke', { id })
export const telegramRevokeAll = () => invoke<RemoteDevice[]>('telegram_revoke_all')
export const telegramSend = (chatId: string, text: string) =>
  invoke<void>('telegram_send', { chatId, text })
export const telegramStart = () => invoke<void>('telegram_start')
export const telegramStop = () => invoke<void>('telegram_stop')

/** Имя ассистента; пустая строка — «как в поставке». */
export const personaName = () => invoke<string>('persona_name')
export const personaSetName = (name: string) => invoke<void>('persona_set_name', { name })
export const profileList = () => invoke<Profile[]>('profile_list')
export const profileSave = (name: string) => invoke<Profile[]>('profile_save', { name })
export const profileApply = (id: string) => invoke<Profile[]>('profile_apply', { id })
export const profileDelete = (id: string) => invoke<Profile[]>('profile_delete', { id })

export const avatarStatus = () => invoke<AvatarStatus>('avatar_status')
export const avatarOpen = () => invoke<AvatarStatus>('avatar_open')
export const avatarClose = () => invoke<void>('avatar_close')
export const avatarSetModel = (path: string) =>
  invoke<AvatarStatus>('avatar_set_model', { path })
export const avatarSetPose = (pose: AvatarPose) =>
  invoke<AvatarStatus>('avatar_set_pose', { pose })
export const avatarSetAnimations = (path: string) =>
  invoke<AvatarStatus>('avatar_set_animations', { path })
export const avatarAnimations = () => invoke<AnimationClip[]>('avatar_animations')
export const avatarAnimationBytes = (name: string) =>
  invoke<ArrayBuffer>('avatar_animation_bytes', { name })
export const avatarPlay = (name: string) => invoke<void>('avatar_play', { name })
export const avatarSetAnchor = (anchor: AvatarAnchor) =>
  invoke<AvatarStatus>('avatar_set_anchor', { anchor })
export const avatarSetClickThrough = (enabled: boolean) =>
  invoke<void>('avatar_set_click_through', { enabled })
export const avatarSetAlwaysOnTop = (enabled: boolean) =>
  invoke<void>('avatar_set_always_on_top', { enabled })
export const avatarRememberPlacement = () => invoke<void>('avatar_remember_placement')

/** Файл модели байтами: его читает Rust, а не WebView. */
export const avatarModelBytes = () => invoke<ArrayBuffer>('avatar_model_bytes')

/** Трансляция состояния во все окна (ТЗ §12). */
export const avatarBroadcast = (signal: AvatarSignal) => emit(AVATAR_EVENT, signal)



export const calendarSetClient = (
  provider: string,
  clientId: string,
  clientSecret?: string,
) =>
  invoke<void>('calendar_set_client', {
    provider,
    clientId,
    clientSecret: clientSecret ?? null,
  })

/** Вход через браузер; ждёт возврата на loopback (ТЗ §25). */
export const calendarConnect = (provider: string) =>
  invoke<CalendarAccount>('calendar_connect', { provider })

export const calendarDisconnect = (provider: string) =>
  invoke<void>('calendar_disconnect', { provider })

export const calendarEvents = (provider: string, from: string, to: string) =>
  invoke<CalendarEvent[]>('calendar_events', { provider, from, to })

export const calendarCreateEvent = (
  provider: string,
  draft: {
    title: string
    start: string
    end: string
    location?: string
    description?: string
  },
) =>
  invoke<CalendarEvent>('calendar_create_event', {
    provider,
    draft: {
      title: draft.title,
      start: draft.start,
      end: draft.end,
      location: draft.location ?? null,
      description: draft.description ?? null,
    },
  })

export const calendarDeleteEvent = (provider: string, id: string) =>
  invoke<void>('calendar_delete_event', { provider, id })

export const pluginList = () => invoke<PluginRecord[]>('plugin_list')

/** Проверка папки до установки (ТЗ §18: permission review). */
export const pluginReview = (path: string) => invoke<PluginReview>('plugin_review', { path })

export const pluginInstall = (args: {
  origin: 'local' | 'dev_folder' | 'git' | 'generated'
  path?: string
  url?: string
}) =>
  invoke<CapabilityRecord>('plugin_install', {
    args: { origin: args.origin, path: args.path ?? null, url: args.url ?? null },
  })

export const pluginRemove = (id: string) => invoke<void>('plugin_remove', { id })

/** Создаёт заготовку плагина и возвращает путь к ней (ТЗ §18). */
export const pluginScaffold = (args: {
  id: string
  name: string
  description?: string
  permissions?: string[]
}) =>
  invoke<string>('plugin_scaffold', {
    args: {
      id: args.id,
      name: args.name,
      description: args.description ?? '',
      permissions: args.permissions ?? [],
    },
  })

/** Где выдать разрешение вручную; null — на этой ОС выдавать нечего (ТЗ §21). */
export const permissionHint = (category: string) =>
  invoke<string | null>('permission_hint', { category })

// ── Память (ТЗ §9) ──────────────────────────────────────────────────────────────

export type MemoryKind = 'short_term' | 'session' | 'long_term' | 'episodic'

export interface MemoryRecord {
  id: string
  kind: MemoryKind
  key: string | null
  content: string
  source: string | null
  confidence: number
  expiresAt: number | null
  createdAt: number
  updatedAt: number
}

export const memoryList = (kind?: MemoryKind) =>
  invoke<MemoryRecord[]>('memory_list', { kind: kind ?? null })

export const memorySave = (entry: {
  kind: MemoryKind
  content: string
  key?: string
  source?: string
  ttlSeconds?: number
}) =>
  invoke<MemoryRecord>('memory_save', {
    kind: entry.kind,
    content: entry.content,
    key: entry.key ?? null,
    source: entry.source ?? null,
    ttlSeconds: entry.ttlSeconds ?? null,
  })

export const memorySearch = (query: string, limit = 20) =>
  invoke<MemoryRecord[]>('memory_search', { query, limit })
export const memoryDelete = (id: string) => invoke<void>('memory_delete', { id })
export const memoryClear = (kind?: MemoryKind) =>
  invoke<number>('memory_clear', { kind: kind ?? null })
/** Память, которая уходит в системную инструкцию перед запросом (ТЗ §9, §24). */
export const memoryContext = () => invoke<string>('memory_context')

// ── Напоминания и уведомления (ТЗ §25) ──────────────────────────────────────────

export interface Reminder {
  id: string
  text: string
  dueAt: number
  recurrence: 'daily' | 'weekly' | null
  completedAt: number | null
  createdAt: number
}

export const reminderCreate = (text: string, dueAt: number, recurrence?: 'daily' | 'weekly') =>
  invoke<Reminder>('reminder_create', { text, dueAt, recurrence: recurrence ?? null })
export const reminderList = (includeCompleted = false) =>
  invoke<Reminder[]>('reminder_list', { includeCompleted })
export const reminderComplete = (id: string) => invoke<void>('reminder_complete', { id })
export const reminderDelete = (id: string) => invoke<void>('reminder_delete', { id })
export const notify = (title: string, body: string) => invoke<void>('notify', { title, body })

// ── Глобальный хоткей (ТЗ §16, §38) ─────────────────────────────────────────────

export const hotkeyGet = () => invoke<string>('hotkey_get')
export const hotkeySet = (shortcut: string) => invoke<void>('hotkey_set', { shortcut })

// ── Голос (ТЗ §10) ──────────────────────────────────────────────────────────────

export type ListenMode = 'push_to_talk' | 'wake_word'

export interface VoiceStatus {
  listening: boolean
  mode: ListenMode | null
  speaking: boolean
  inputDevice: string | null
  devices: string[]
  voices: string[]
  /** Чем говорит: `system` или `http`. */
  engine: 'system' | 'http'
  /** Отдаёт ли движок громкость — от этого зависит, настоящий ли lip-sync. */
  hasLevel: boolean
  /** Настроено ли распознавание: без него голосовой ввод невозможен. */
  sttReady: boolean
}

export const voiceStatus = () => invoke<VoiceStatus>('voice_status')
/** Громкость речи 0…1; `null` — движок звука не отдаёт. */
export const voiceSpeakingLevel = () => invoke<number | null>('voice_speaking_level')
export const voiceStart = (mode: ListenMode) => invoke<void>('voice_start', { mode })
export const voiceStop = () => invoke<void>('voice_stop')
/** Отпущена кнопка push-to-talk: отдать накопленное, не дожидаясь паузы. */
export const voiceFinishUtterance = () => invoke<void>('voice_finish_utterance')
export const voiceSpeak = (text: string) => invoke<void>('voice_speak', { text })
/** Замолчать немедленно — перебивание из ТЗ §10. */
export const voiceStopSpeaking = () => invoke<void>('voice_stop_speaking')
export const voiceSetVoice = (name: string) => invoke<void>('voice_set_voice', { name })
export const voiceConfigureStt = (settings: {
  providerId?: string
  model?: string
  language?: string
}) =>
  invoke<void>('voice_configure_stt', {
    settings: {
      providerId: settings.providerId ?? null,
      model: settings.model ?? null,
      language: settings.language ?? null,
    },
  })

// ── Возможности и MCP (ТЗ §17, §18, §19) ────────────────────────────────────────

export interface Integration {
  id: string
  label: string
  description: string
  transport: string
  command: string
  args: string[]
  secretEnv: string | null
  secretHint: string | null
  permissions: string[]
  /** Поддерживается сообществом, а не авторами протокола. */
  community: boolean
}

export interface CapabilityRecord {
  id: string
  name: string
  description: string
  version: string
  source: 'builtin' | 'mcp' | 'plugin' | 'user_script' | 'api'
  enabled: boolean
  health: 'ok' | 'degraded' | 'failed' | 'unknown'
  healthNote: string | null
  tools: string[]
  permissions: string[]
  installedAt: number
}

export interface McpToolSpec {
  id: string
  serverId: string
  toolName: string
  description: string
  inputSchema: Record<string, unknown>
}

export const integrationsList = (query?: string) =>
  invoke<Integration[]>('integrations_list', { query: query ?? null })
export const integrationInstall = (id: string, secret?: string) =>
  invoke<CapabilityRecord>('integration_install', { id, secret: secret ?? null })

export const capabilityList = () => invoke<CapabilityRecord[]>('capability_list')
export const capabilitySetEnabled = (id: string, enabled: boolean) =>
  invoke<void>('capability_set_enabled', { id, enabled })
export const capabilityRemove = (id: string) => invoke<void>('capability_remove', { id })

export const mcpAdd = (server: {
  id: string
  label: string
  transport: 'stdio' | 'http' | 'sse'
  command?: string
  args?: string[]
  url?: string
  env?: Record<string, string>
  secret?: string
  secretEnv?: string
}) =>
  invoke<CapabilityRecord>('mcp_add', {
    server: {
      id: server.id,
      label: server.label,
      transport: server.transport,
      command: server.command ?? null,
      args: server.args ?? [],
      url: server.url ?? null,
      env: server.env ?? {},
      secret: server.secret ?? null,
      secretEnv: server.secretEnv ?? null,
    },
  })

/** Test Connection: подключается заново и возвращает список инструментов (ТЗ §19). */
export const mcpTest = (id: string) => invoke<string[]>('mcp_test', { id })
export const mcpTools = () => invoke<McpToolSpec[]>('mcp_tools')
export const mcpCall = (serverId: string, tool: string, args: unknown) =>
  invoke<string>('mcp_call', { serverId, tool, arguments: args })

// ── Команды и автоматизации (ТЗ §16) ────────────────────────────────────────────

export interface CommandRecord {
  id: string
  name: string
  description: string
  triggerKind: 'phrase' | 'hotkey' | 'startup' | 'manual'
  phrase: string | null
  hotkey: string | null
  enabled: boolean
  /** Шаги программы; структура описана в @yuki/core. */
  steps: unknown[]
}

export const commandList = () => invoke<CommandRecord[]>('command_list')
export const commandSave = (command: CommandRecord) => invoke<void>('command_save', { command })
export const commandDelete = (id: string) => invoke<void>('command_delete', { id })
