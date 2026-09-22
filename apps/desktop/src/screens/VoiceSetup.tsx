import { useEffect, useState } from 'react'
import { settingGet, voiceConfigureStt, voiceSetVoice, voiceSpeak, voiceStatus, type VoiceStatus } from '../bridge'

const PRESETS = {
  shared: { label: 'Ключ OpenAI из подключения модели', url: '', model: 'whisper-1' },
  openai: { label: 'OpenAI · отдельный ключ', url: 'https://api.openai.com/v1', model: 'whisper-1' },
  groq: { label: 'Groq · отдельный ключ', url: 'https://api.groq.com/openai/v1', model: 'whisper-large-v3-turbo' },
  local: { label: 'Локальный сервер · без ключа', url: 'http://127.0.0.1:8000/v1', model: 'whisper-1' },
}

export function VoiceSetup() {
  const [preset, setPreset] = useState<keyof typeof PRESETS>('shared')
  const [url, setUrl] = useState('')
  const [model, setModel] = useState('whisper-1')
  const [language, setLanguage] = useState('ru')
  const [key, setKey] = useState('')
  const [status, setStatus] = useState<VoiceStatus | null>(null)
  const [voice, setVoice] = useState('')
  const [note, setNote] = useState('')
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let mounted = true
    Promise.all([settingGet('voice.stt.url'), settingGet('voice.stt.model'), settingGet('voice.language'), voiceStatus(), settingGet('voice.tts.voice')])
      .then(([savedUrl, savedModel, lang, state, savedVoice]) => {
        if (!mounted) return
        setUrl(savedUrl ?? '')
        setPreset(savedUrl?.includes('groq.com') ? 'groq' : savedUrl?.includes('openai.com') ? 'openai' : savedUrl ? 'local' : 'shared')
        setModel(savedModel || 'whisper-1'); setLanguage(lang || 'ru'); setStatus(state)
        setVoice(savedVoice ?? '')
      }).catch(() => { if (mounted) setNote('Настройка голоса доступна в установленном приложении.') })
    return () => { mounted = false }
  }, [])

  async function save() {
    setBusy(true); setNote('')
    try {
      await voiceConfigureStt({ baseUrl: url.trim(), model: model.trim(), language, ...(key.trim() ? { apiKey: key.trim() } : {}) })
      setKey(''); const next = await voiceStatus(); setStatus(next)
      setNote(next.sttReady ? 'Сохранено. Нажмите микрофон и произнесите короткую фразу, чтобы проверить распознавание.' : 'Сохранено. Для выбранного сервиса ещё нужен ключ или подключение OpenAI.')
    } catch (error) { setNote(String(error)) } finally { setBusy(false) }
  }

  return <section className="settings__section voice-setup">
    <header className="settings__header">
      <h2 className="settings__title">Давайте поговорим</h2>
      <p className="settings__hint">Модель отвечает, распознавание превращает ваш голос в текст, а системный голос озвучивает ответ. Подписки Claude и ChatGPT через CLI отвечают за текст; распознавание подключается отдельно.</p>
    </header>
    <label className="setup-field">Распознавание речи
      <select className="settings__input" value={preset} onChange={e => {
        const choice = e.target.value as keyof typeof PRESETS
        setPreset(choice); setUrl(PRESETS[choice].url); setModel(PRESETS[choice].model); setKey(''); setNote('')
      }}>{Object.entries(PRESETS).map(([id, item]) => <option key={id} value={id}>{item.label}</option>)}</select>
    </label>
    {preset !== 'shared' && <>
      <label className="setup-field">Адрес сервиса <input className="settings__input" value={url} onChange={e => setUrl(e.target.value)} spellCheck={false} /></label>
      {preset !== 'local' && <label className="setup-field">Ключ распознавания <input type="password" autoComplete="off" className="settings__input" value={key} onChange={e => setKey(e.target.value)} placeholder="Пустое поле сохраняет текущий ключ" /></label>}
    </>}
    {preset === 'local' && <p className="settings__hint">Свой сервер с эндпоинтом /v1/audio/transcriptions. Ollama и LM Studio сами по себе не распознают речь. Если вы включили распознавание на этом компьютере выше, адрес уже подставлен и менять здесь ничего не нужно.</p>}
    <div className="provider__row">
      <label className="setup-field">Модель <input className="settings__input" value={model} onChange={e => setModel(e.target.value)} /></label>
      <label className="setup-field">Язык <select className="settings__input" value={language} onChange={e => setLanguage(e.target.value)}><option value="ru">Русский</option><option value="en">English</option><option value="uk">Українська</option></select></label>
    </div>
    <div className="provider__row"><button className="settings__button" disabled={busy || !model.trim()} onClick={() => void save()}>{busy ? 'Сохраняю…' : 'Сохранить голос'}</button>
      <button className="settings__button" disabled={busy} onClick={() => void voiceSpeak('Привет! Я Юки. Буду рядом, когда понадоблюсь.').catch(e => setNote(String(e)))}>Послушать Юки</button></div>
    {!!status?.voices.length && <label className="setup-field">Системный голос<select className="settings__input" value={voice} onChange={e => { const next = e.target.value; void voiceSetVoice(next).then(() => setVoice(next)).catch(err => setNote(String(err))) }}><option value="" disabled>Голос по умолчанию</option>{status.voices.map(v => <option key={v}>{v}</option>)}</select></label>}
    {status && <p className="settings__hint">{status.inputDevice ? `Микрофон: ${status.inputDevice}` : 'Микрофон не обнаружен'}. {status.sttReady ? 'Распознавание настроено; проверка фразой ещё необходима.' : 'Распознавание ещё не настроено.'}</p>}
    {note && <p className="provider__status" role="status">{note}</p>}
  </section>
}
