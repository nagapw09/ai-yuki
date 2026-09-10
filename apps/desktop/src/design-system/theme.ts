/**
 * Тема оформления (`docs/GAPS.md` §8).
 *
 * ТЗ §13 требует dark-first, и тёмная остаётся базой: светлая — это вариант
 * для тех, кому тёмный интерфейс физически тяжёл, а не равноправная вторая
 * шкурка. Поэтому умолчание — `dark`, а не системная тема: ассистент, который
 * при первом запуске выглядит по-разному у двух людей, теряет собственное лицо.
 *
 * Вариант `system` есть отдельно и делает ровно то, что обещает, — следит за
 * настройкой ОС, в том числе когда она меняется по расписанию дня.
 */

import { isTauri, settingGet, settingSet } from '../bridge'

export type ThemeMode = 'dark' | 'light' | 'system'

/** Ключ настройки. */
const SETTING_KEY = 'ui.theme'

const DEFAULT: ThemeMode = 'dark'

export function isThemeMode(value: string): value is ThemeMode {
  return value === 'dark' || value === 'light' || value === 'system'
}

/** Медиазапрос системной темы; `null` там, где его нет. */
function systemQuery(): MediaQueryList | null {
  if (typeof window === 'undefined' || !window.matchMedia) return null
  return window.matchMedia('(prefers-color-scheme: light)')
}

/**
 * Проставляет атрибут темы на корне документа.
 *
 * Тёмная не помечается ничем: она задана в `:root`, и лишний атрибут заставил
 * бы дублировать всю палитру ради значения по умолчанию.
 */
export function applyTheme(mode: ThemeMode): void {
  if (typeof document === 'undefined') return

  const light = mode === 'light' || (mode === 'system' && (systemQuery()?.matches ?? false))

  if (light) {
    document.documentElement.dataset.theme = 'light'
  } else {
    delete document.documentElement.dataset.theme
  }
}

/**
 * Читает сохранённую тему и начинает её применять.
 *
 * Возвращает функцию отписки: в режиме `system` подписка на медиазапрос живёт,
 * пока живёт приложение, и оставлять её висеть при размонтировании нельзя.
 */
export function startTheme(): () => void {
  let mode: ThemeMode = DEFAULT
  const query = systemQuery()

  const onSystemChange = () => applyTheme(mode)

  applyTheme(mode)
  query?.addEventListener('change', onSystemChange)

  if (isTauri()) {
    void settingGet(SETTING_KEY)
      .then((saved) => {
        if (saved && isThemeMode(saved)) {
          mode = saved
          applyTheme(mode)
        }
      })
      .catch(() => undefined)
  }

  // Смена темы из настроек должна отражаться сразу, а не после перезапуска.
  const onManualChange = (event: Event) => {
    const next = (event as CustomEvent<ThemeMode>).detail
    if (isThemeMode(next)) {
      mode = next
      applyTheme(mode)
    }
  }

  window.addEventListener(THEME_EVENT, onManualChange)

  return () => {
    query?.removeEventListener('change', onSystemChange)
    window.removeEventListener(THEME_EVENT, onManualChange)
  }
}

/** Событие смены темы внутри окна. */
const THEME_EVENT = 'yuki:theme'

/** Сохраняет выбор и применяет его немедленно. */
export async function setTheme(mode: ThemeMode): Promise<void> {
  window.dispatchEvent(new CustomEvent<ThemeMode>(THEME_EVENT, { detail: mode }))
  if (isTauri()) await settingSet(SETTING_KEY, mode)
}

/** Текущий сохранённый режим. */
export async function getTheme(): Promise<ThemeMode> {
  if (!isTauri()) return DEFAULT
  const saved = await settingGet(SETTING_KEY).catch(() => null)
  return saved && isThemeMode(saved) ? saved : DEFAULT
}
