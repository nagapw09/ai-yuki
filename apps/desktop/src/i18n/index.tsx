import { createContext, useContext, useEffect, useMemo, useState } from 'react'
import type { ReactNode } from 'react'

import { isTauri, settingGet, settingSet } from '../bridge'

import { en } from './en'
import { ru } from './ru'
import { uk } from './uk'

/** Языки первого релиза (ТЗ §36). */
export type Locale = 'ru' | 'en' | 'uk'

export type Dictionary = Record<string, string>

const DICTIONARIES: Record<Locale, Dictionary> = { ru, en, uk }

/** Русский — язык по умолчанию, если системный не входит в поддерживаемые. */
const FALLBACK: Locale = 'ru'

/** Ключ настройки языка. */
const SETTING_KEY = 'ui.language'

function isLocale(value: string): value is Locale {
  return value === 'ru' || value === 'en' || value === 'uk'
}

interface I18nValue {
  locale: Locale
  setLocale: (locale: Locale) => void
  t: (key: string, vars?: Record<string, string | number>) => string
}

const I18nContext = createContext<I18nValue | null>(null)

/** Определяет язык по настройкам ОС; неизвестный язык уводит на fallback. */
export function detectLocale(): Locale {
  const raw = typeof navigator === 'undefined' ? '' : navigator.language.toLowerCase()
  if (raw.startsWith('uk')) return 'uk'
  if (raw.startsWith('en')) return 'en'
  if (raw.startsWith('ru')) return 'ru'
  return FALLBACK
}

export function I18nProvider({
  children,
  initialLocale,
}: {
  children: ReactNode
  initialLocale?: Locale
}) {
  const [locale, setLocale] = useState<Locale>(initialLocale ?? detectLocale())

  // Язык, выбранный в мастере, должен переживать перезапуск: иначе при
  // каждом старте он определяется по системному и тихо отменяет выбор.
  useEffect(() => {
    if (initialLocale || !isTauri()) return

    let cancelled = false
    void settingGet(SETTING_KEY)
      .then((saved) => {
        if (!cancelled && saved && isLocale(saved)) setLocale(saved)
      })
      .catch(() => undefined)

    return () => {
      cancelled = true
    }
  }, [initialLocale])

  const value = useMemo<I18nValue>(() => {
    const dict = DICTIONARIES[locale]
    const fallbackDict = DICTIONARIES[FALLBACK]

    return {
      locale,
      setLocale: (next: Locale) => {
        setLocale(next)
        if (isTauri()) void settingSet(SETTING_KEY, next).catch(() => undefined)
      },
      t: (key, vars) => {
        // Отсутствующий перевод не должен ломать экран: сначала пробуем язык
        // по умолчанию, и только потом показываем сам ключ — так пропуск виден
        // при разработке, но пользователь всё равно получает осмысленный текст.
        const template = dict[key] ?? fallbackDict[key] ?? key
        if (!vars) return template
        return template.replace(/\{(\w+)\}/g, (match, name: string) =>
          name in vars ? String(vars[name]) : match,
        )
      },
    }
  }, [locale])

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>
}

export function useI18n(): I18nValue {
  const value = useContext(I18nContext)
  if (!value) throw new Error('useI18n вызван вне I18nProvider')
  return value
}

export function useT() {
  return useI18n().t
}
