import { useRef, useState } from 'react'
import type { FormEvent, KeyboardEvent } from 'react'

import { useT } from '../../i18n'
import './CommandBar.css'

export interface CommandBarProps {
  onSubmit: (text: string) => void
  onToggleVoice: () => void
  listening: boolean
  disabled?: boolean
  placeholder?: string
}

export function CommandBar({
  onSubmit,
  onToggleVoice,
  listening,
  disabled = false,
  placeholder,
}: CommandBarProps) {
  const t = useT()
  const [value, setValue] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const text = value.trim()
    if (!text || disabled) return
    onSubmit(text)
    setValue('')
  }

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    // Escape очищает поле, а не закрывает окно: пользователь передумал
    // формулировку, а не решил уйти из приложения.
    if (event.key === 'Escape' && value) {
      event.stopPropagation()
      setValue('')
    }
  }

  return (
    <form className="command-bar" data-listening={listening} onSubmit={submit}>
      <input
        ref={inputRef}
        className="command-bar__input"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder ?? t('commandBar.placeholder')}
        disabled={disabled}
        aria-label={t('commandBar.label')}
        autoComplete="off"
        spellCheck={false}
      />

      {value.length === 0 && (
        <span className="command-bar__hint">{t('commandBar.hint')}</span>
      )}

      <button
        type="button"
        className="command-bar__button command-bar__button--mic"
        data-active={listening}
        onClick={onToggleVoice}
        disabled={disabled}
        aria-pressed={listening}
        aria-label={listening ? t('commandBar.stopVoice') : t('commandBar.startVoice')}
      >
        <MicIcon />
      </button>

      <button
        type="submit"
        className="command-bar__button command-bar__button--send"
        disabled={disabled || value.trim().length === 0}
        aria-label={t('commandBar.send')}
      >
        <SendIcon />
      </button>
    </form>
  )
}

function MicIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <rect x="9" y="3" width="6" height="11" rx="3" fill="currentColor" />
      <path
        d="M5 11a7 7 0 0 0 14 0M12 18v3"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
      />
    </svg>
  )
}

function SendIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path
        d="M5 12h13M12 5l7 7-7 7"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}
