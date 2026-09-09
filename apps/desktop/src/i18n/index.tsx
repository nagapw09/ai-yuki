import { createContext, useContext, useMemo, useState } from 'react'
import type { ReactNode } from 'react'

import { en } from './en'
import { ru } from './ru'
import { uk } from './uk'

/** Языки первого релиза (ТЗ §36). */
export type Locale = 'ru' | 'en' | 'uk'

export type Dictionary = Record<string, string>

const DICTIONARIES: Record<Locale, Dictionary> = { ru, en, uk }

/** Русский — язык по умолчанию, если системный не входит в поддерживаемые. */
const FALLBACK: Locale = 'ru'

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

  const value = useMemo<I18nValue>(() => {
    const dict = DICTIONARIES[locale]
    const fallbackDict = DICTIONARIES[FALLBACK]

    return {
      locale,
      setLocale,
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
