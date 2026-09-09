import { useCallback, useEffect, useState } from 'react'

import { startVoice, stopVoice } from '../agent/voice'
import { useUiStore } from '../state/store'
import {
  avatarClose,
  avatarOpen,
  avatarSetAlwaysOnTop,
  avatarSetClickThrough,
  avatarSetModel,
  avatarStatus,
  calendarAccounts,
  calendarConnect,
  calendarDisconnect,
  calendarSetClient,
  hotkeyGet,
  hotkeySet,
  openUrl,
  permissionHint,
  permissionSet,
  permissionsList,
  privacySetLocalOnly,
  privacyStatus,
  providerClearKey,
  providerList,
  providerSave,
  providerSetDefault,
  providerSetKey,
  providerTest,
  settingGet,
  voiceConfigureStt,
  voiceSetVoice,
  voiceStatus,
  type AvatarStatus,
  type CalendarAccount,
  type PermissionStatus,
  type PrivacyStatus,
  type ProviderRecord,
  type VoiceStatus as VoiceStatusRecord,
} from '../bridge'
import './Settings.css'

/** Подписи категорий разрешений из ТЗ §21. */
const PERMISSION_LABEL: Record<string, string> = {
  microphone: 'Микрофон',
  screen_recording: 'Запись экрана',
  accessibility: 'Управление интерфейсом',
  files: 'Файлы',
  network: 'Сеть',
  shell: 'Терминал',
  camera: 'Камера',
  notifications: 'Уведомления',
  browser: 'Браузер',
  external_services: 'Внешние сервисы',
}

export function Settings() {
  return (
    <div className="settings">
      <div className="settings__inner">
        <CapabilitiesEntry />
        <Privacy />
        <Providers />
        <Voice />
        <Calendar />
        <Avatar />
        <Hotkey />
        <Permissions />
      </div>
    </div>
  )
}

// ── Вход в Capability Hub (ТЗ §17) ──────────────────────────────────────────────

function CapabilitiesEntry() {
  const setScreen = useUiStore((s) => s.setScreen)

  return (
    <section className="settings__section">
      <header className="settings__header">
        <h2 className="settings__title">Возможности</h2>
        <p className="settings__hint">
          Интеграции и MCP-серверы, которые расширяют то, что Yuki умеет.
          Можно просто попросить её: «добавь возможность управлять…».
        </p>
      </header>
      <button
        type="button"
        className="settings__button"
        onClick={() => setScreen('capabilities')}
      >
        Открыть Capability Hub
      </button>
    </section>
  )
}

// ── Голос (ТЗ §10) ──────────────────────────────────────────────────────────────

function Voice() {
  const [status, setStatus] = useState<VoiceStatusRecord | null>(null)
  const [model, setModel] = useState('')
  const [language, setLanguage] = useState('')
  const [note, setNote] = useState<string | null>(null)

  const reload = useCallback(async () => {
    try {
      setStatus(await voiceStatus())
    } catch (e) {
      setNote(describe(e))
    }
  }, [])

  useEffect(() => {
    void reload()
    settingGet('voice.stt.model').then((v) => setModel(v ?? '')).catch(() => undefined)
    settingGet('voice.language').then((v) => setLanguage(v ?? '')).catch(() => undefined)
  }, [reload])

  const save = () => {
    void voiceConfigureStt({ model, language })
      .then(() => {
        setNote('сохранено')
        return reload()
      })
      .catch((e: unknown) => setNote(describe(e)))
  }

  return (
    <section className="settings__section">
      <header className="settings__header">
        <h2 className="settings__title">Голос</h2>
        <p className="settings__hint">
          Речь синтезирует сама операционная система — без ключей и без сети.
          Для распознавания нужен провайдер с протоколом OpenAI: облачный или
          локальный Whisper на своём порту.
        </p>
      </header>

      {status && !status.sttReady && (
        // Прямая причина вместо молчания: без этого кнопка микрофона просто
        // не работала бы, и понять почему было бы неоткуда.
        <p className="settings__error">
          Распознавание не настроено — включите провайдера протокола OpenAI выше.
        </p>
      )}

      <div className="provider__row">
        <input
          className="settings__input"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          onBlur={save}
          placeholder="Модель распознавания (по умолчанию whisper-1)"
          spellCheck={false}
        />
        <input
          className="settings__input"
          value={language}
          onChange={(e) => setLanguage(e.target.value)}
          onBlur={save}
          placeholder="Язык: ru, en, uk"
          spellCheck={false}
          style={{ maxWidth: 160 }}
        />
      </div>

      {status && status.voices.length > 0 && (
        <div className="provider__row">
          <select
            className="settings__input"
            defaultValue=""
            onChange={(e) => {
              if (!e.target.value) return
              void voiceSetVoice(e.target.value)
                .then(() => setNote('голос выбран'))
                .catch((err: unknown) => setNote(describe(err)))
            }}
          >
            <option value="">Голос Yuki — выбрать…</option>
            {status.voices.map((voice) => (
              <option key={voice} value={voice}>
                {voice}
              </option>
            ))}
          </select>
        </div>
      )}

      <div className="provider__row">
        <button
          type="button"
          className="settings__button"
          disabled={!status?.sttReady}
          onClick={() => {
            // Сообщение прошлой попытки надо убрать до новой: иначе рядом
            // с успешно включённым микрофоном висит старая причина отказа.
            setNote(null)
            const action = status?.listening
              ? stopVoice()
              : startVoice('wake_word').catch((e: unknown) => setNote(describe(e)))
            void Promise.resolve(action).then(reload)
          }}
        >
          {status?.listening ? 'Выключить прослушивание' : 'Слушать постоянно («Юки, …»)'}
        </button>
        <span className="provider__status">
          {status?.inputDevice ? `микрофон: ${status.inputDevice}` : 'микрофон не найден'}
          {note ? ` · ${note}` : ''}
        </span>
      </div>

      <p className="settings__hint" style={{ marginTop: 'var(--space-3)' }}>
        Постоянное прослушивание распознаёт фразу целиком и только потом ищет
        в ней обращение, поэтому отзыв наступает после того, как вы договорили.
        Мгновенная реакция требует отдельной модели пробуждения — она в планах.
      </p>
    </section>
  )
}

// ── Календари (ТЗ §25) ──────────────────────────────────────────

/**
 * Подключение Google Calendar и Outlook.
 *
 * Два шага, и первый нельзя пропустить: человек заводит своё
 * приложение в консоли сервиса и вставляет сюда client_id, потом входит
 * через браузер. Вшить учётные данные в открытое приложение нельзя —
 * они мгновенно перестают быть его учётными данными.
 */
function Calendar() {
  const [accounts, setAccounts] = useState<CalendarAccount[]>([])
  const [error, setError] = useState<string | null>(null)

  const reload = useCallback(async () => {
    try {
      setAccounts(await calendarAccounts())
      setError(null)
    } catch (e) {
      setError(describe(e))
    }
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  return (
    <section className="settings__section">
      <h3 className="settings__title">Календарь</h3>
      <p className="settings__hint">
        Чтобы Юки видела ваши встречи и могла их создавать, нужно своё
        приложение в консоли сервиса — типа «Desktop» или «Mobile and desktop».
        Вход откроется в обычном браузере: форма входа внутри чужого окна
        неотличима от подделки, и сервисы её запрещают.
      </p>

      {error && <p className="settings__error">{error}</p>}

      <div className="settings__list">
        {accounts.map((account) => (
          <CalendarRow key={account.provider} account={account} onChanged={reload} />
        ))}
      </div>
    </section>
  )
}

function CalendarRow({
  account,
  onChanged,
}: {
  account: CalendarAccount
  onChanged: () => Promise<void>
}) {
  const [clientId, setClientId] = useState(account.clientId)
  const [clientSecret, setClientSecret] = useState('')
  const [status, setStatus] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const run = async (action: () => Promise<string | null>) => {
    setBusy(true)
    try {
      setStatus(await action())
      await onChanged()
    } catch (e) {
      setStatus(describe(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="provider" data-default={account.connected}>
      <div className="provider__head">
        <span className="provider__label">{account.label}</span>
        {account.connected && <span className="provider__badge">подключён</span>}
        <button
          type="button"
          className="settings__link"
          onClick={() => void openUrl(account.consoleUrl)}
        >
          консоль сервиса
        </button>
      </div>

      <div className="provider__row">
        <input
          className="settings__input"
          value={clientId}
          onChange={(e) => setClientId(e.target.value)}
          placeholder="client_id"
          spellCheck={false}
        />
        <input
          className="settings__input"
          type="password"
          value={clientSecret}
          onChange={(e) => setClientSecret(e.target.value)}
          placeholder="client_secret, если выдан"
          spellCheck={false}
          autoComplete="off"
        />
        <button
          type="button"
          className="settings__button"
          disabled={busy || clientId.trim() === ''}
          onClick={() =>
            void run(async () => {
              await calendarSetClient(account.provider, clientId, clientSecret || undefined)
              setClientSecret('')
              return 'сохранено'
            })
          }
        >
          Сохранить
        </button>
      </div>

      <div className="provider__row">
        {account.connected ? (
          <button
            type="button"
            className="settings__button"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                await calendarDisconnect(account.provider)
                return 'отключён'
              })
            }
          >
            Отключить
          </button>
        ) : (
          <button
            type="button"
            className="settings__button"
            disabled={busy || account.clientId === ''}
            onClick={() =>
              void run(async () => {
                await calendarConnect(account.provider)
                return 'вход выполнен'
              })
            }
          >
            {busy ? 'Жду браузер…' : 'Войти через браузер'}
          </button>
        )}
        {status && <span className="provider__status">{status}</span>}
      </div>
    </div>
  )
}

// ── Аватар (ТЗ §12) ─────────────────────────────────────────────

/**
 * Окно аватара.
 *
 * Модель приносит пользователь: у VRM свои лицензии, и класть чужую
 * в дистрибутив нельзя. Без модели окно всё равно работает и показывает
 * те же восемь состояний свечением.
 */
function Avatar() {
  const [status, setStatus] = useState<AvatarStatus | null>(null)
  const [model, setModel] = useState('')
  const [note, setNote] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const reload = useCallback(async () => {
    try {
      const next = await avatarStatus()
      setStatus(next)
      setModel(next.model)
      setNote(null)
    } catch (e) {
      setNote(describe(e))
    }
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  const run = (action: () => Promise<string | null>) => {
    setBusy(true)
    action()
      .then((message) => {
        setNote(message)
        return reload()
      })
      .catch((e: unknown) => setNote(describe(e)))
      .finally(() => setBusy(false))
  }

  if (!status) {
    return null
  }

  return (
    <section className="settings__section">
      <h3 className="settings__title">Аватар</h3>
      <p className="settings__hint">
        Отдельное окно без рамки поверх остальных. Модель — файл .vrm; в поставке
        её нет, потому что у моделей свои лицензии. Без модели окно покажет
        свечение в тех же восьми состояниях.
      </p>

      <div className="provider__row">
        <input
          className="settings__input"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          placeholder="Путь к файлу .vrm"
          spellCheck={false}
        />
        <button
          type="button"
          className="settings__button"
          disabled={busy}
          onClick={() =>
            run(async () => {
              await avatarSetModel(model)
              return model.trim() === '' ? 'модель убрана' : 'модель сохранена'
            })
          }
        >
          Сохранить
        </button>
      </div>

      <div className="provider__row">
        <button
          type="button"
          className="settings__button"
          disabled={busy}
          onClick={() =>
            run(async () => {
              if (status.open) {
                await avatarClose()
                return 'закрыт'
              }
              await avatarOpen()
              return 'открыт'
            })
          }
        >
          {status.open ? 'Закрыть аватар' : 'Показать аватар'}
        </button>

        <label className="hub__checkbox">
          <input
            type="checkbox"
            checked={status.alwaysOnTop}
            disabled={busy}
            onChange={(e) =>
              run(async () => {
                await avatarSetAlwaysOnTop(e.target.checked)
                return null
              })
            }
          />
          поверх всех окон
        </label>

        <label className="hub__checkbox">
          <input
            type="checkbox"
            checked={status.clickThrough}
            disabled={busy}
            onChange={(e) =>
              run(async () => {
                await avatarSetClickThrough(e.target.checked)
                return null
              })
            }
          />
          {/* Сквозной режим забирает у окна все клики — включая перетаскивание. */}
          пропускать клики насквозь
        </label>

        {note && <span className="provider__status">{note}</span>}
      </div>

      {status.model !== '' && !status.modelPresent && (
        <p className="settings__error">
          Файл модели не найден по сохранённому пути — возможно, его переместили.
        </p>
      )}
    </section>
  )
}

// ── Глобальный хоткей (ТЗ §16, §38) ─────────────────────────────────────────────

/**
 * Сочетание собирается по `event.code`, а не по `event.key`.
 *
 * `key` зависит от раскладки: на русской та же клавиша даёт «н» вместо «y», и
 * записанный по ней хоткей перестаёт работать при переключении языка. `code`
 * описывает физическую клавишу и от раскладки не зависит.
 */
function shortcutFromEvent(event: React.KeyboardEvent): string | null {
  const code = event.code

  // Одни модификаторы сочетанием не являются.
  if (/^(Control|Shift|Alt|Meta|OS)/.test(code)) return null

  const parts: string[] = []
  if (event.ctrlKey) parts.push('Ctrl')
  if (event.shiftKey) parts.push('Shift')
  if (event.altKey) parts.push('Alt')
  if (event.metaKey) parts.push('Super')

  // Хоткей без модификатора перехватил бы клавишу во всех приложениях сразу.
  if (parts.length === 0) return null

  const key = code.startsWith('Key')
    ? code.slice(3)
    : code.startsWith('Digit')
      ? code.slice(5)
      : code

  parts.push(key)
  return parts.join('+')
}

function Hotkey() {
  const [shortcut, setShortcut] = useState('')
  const [capturing, setCapturing] = useState(false)
  const [status, setStatus] = useState<string | null>(null)

  useEffect(() => {
    hotkeyGet().then(setShortcut).catch(() => undefined)
  }, [])

  return (
    <section className="settings__section">
      <header className="settings__header">
        <h2 className="settings__title">Вызов Yuki</h2>
        <p className="settings__hint">
          Сочетание работает поверх любого приложения. Повторное нажатие прячет окно.
        </p>
      </header>

      <div className="provider__row">
        <button
          type="button"
          className="settings__input settings__capture"
          data-capturing={capturing}
          onClick={() => {
            setCapturing(true)
            setStatus(null)
          }}
          onBlur={() => setCapturing(false)}
          onKeyDown={(event) => {
            if (!capturing) return
            event.preventDefault()

            if (event.key === 'Escape') {
              setCapturing(false)
              return
            }

            const next = shortcutFromEvent(event)
            if (!next) return

            setCapturing(false)
            // Проверку делает Rust: он единственный знает, удалось ли занять
            // сочетание в системе. Показываем то, что он ответил.
            hotkeySet(next)
              .then(() => {
                setShortcut(next)
                setStatus('сохранено')
              })
              .catch((e: unknown) => setStatus(describe(e)))
          }}
        >
          {capturing ? 'Нажмите сочетание…' : shortcut || 'не задано'}
        </button>
        {status && <span className="provider__status">{status}</span>}
      </div>
    </section>
  )
}

// ── Local Only (ТЗ §29) ────────────────────────────────────────────────────

/**
 * Режим «только локально».
 *
 * Секция говорит две вещи и не больше: что режим гарантирует и что ему
 * мешает сейчас. Включённый режим при облачном провайдере — это не
 * ошибка настройки, а блокировка запросов, и человек должен видеть это
 * до того, как получит отказ в ответ на реплику.
 */
function Privacy() {
  const [status, setStatus] = useState<PrivacyStatus | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const reload = useCallback(async () => {
    try {
      setStatus(await privacyStatus())
      setError(null)
    } catch (e) {
      setError(describe(e))
    }
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  const toggle = () => {
    if (!status) return
    setBusy(true)
    privacySetLocalOnly(!status.localOnly)
      .then(setStatus)
      .catch((e: unknown) => setError(describe(e)))
      .finally(() => setBusy(false))
  }

  return (
    <section className="settings__section">
      <h3 className="settings__title">Приватность</h3>
      <p className="settings__hint">
        В режиме Local Only ни одна реплика и ни одна запись голоса не уходят
        с машины: разрешены только модель и распознавание на localhost —
        например Ollama или LM Studio. Память и синтез речи локальны всегда.
        Режим не отключает сеть целиком: включённые вами MCP-серверы и
        открытие ссылок продолжают работать.
      </p>

      {error && <p className="settings__error">{error}</p>}

      {status && (
        <>
          <div className="provider__row">
            <button type="button" className="settings__button" disabled={busy} onClick={toggle}>
              {status.localOnly ? 'Выключить Local Only' : 'Включить Local Only'}
            </button>
            <span className="provider__status">
              {status.localOnly ? 'режим включён' : 'режим выключен'}
            </span>
          </div>

          {status.blockers.length > 0 && (
            /* Помехи показываются и при выключенном режиме: так видно
               заранее, что придётся поменять, а не после первого отказа. */
            <ul className="settings__blockers">
              {status.blockers.map((blocker) => (
                <li key={blocker} className="settings__blocker">
                  {status.localOnly ? 'заблокировано: ' : 'помешает: '}
                  {blocker}
                </li>
              ))}
            </ul>
          )}

          {status.localOnly && status.blockers.length === 0 && (
            <p className="settings__hint">
              Всё локально: {status.providerLabel ?? 'модель'}
              {status.sttUrl === null ? '' : ', распознавание на ' + status.sttUrl}.
            </p>
          )}
        </>
      )}
    </section>
  )
}

// ── Провайдеры (ТЗ §4) ────────────────────────────────────────────────────

function Providers() {
  const [providers, setProviders] = useState<ProviderRecord[]>([])
  const [error, setError] = useState<string | null>(null)

  const reload = useCallback(async () => {
    try {
      setProviders(await providerList())
      setError(null)
    } catch (e) {
      setError(describe(e))
    }
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  return (
    <section className="settings__section">
      <header className="settings__header">
        <h2 className="settings__title">AI-провайдер</h2>
        <p className="settings__hint">
          Ключ хранится в системном хранилище — Keychain на macOS, Диспетчер учётных
          данных на Windows. В базу Yuki он не попадает и во фронтенд не возвращается.
        </p>
      </header>

      {error && <p className="settings__error">{error}</p>}

      <div className="settings__list">
        {providers.map((provider) => (
          <ProviderRow key={provider.id} provider={provider} onChanged={reload} />
        ))}
      </div>
    </section>
  )
}

function ProviderRow({
  provider,
  onChanged,
}: {
  provider: ProviderRecord
  onChanged: () => Promise<void>
}) {
  const [key, setKey] = useState('')
  const [model, setModel] = useState(provider.defaultModel)
  const [baseUrl, setBaseUrl] = useState(provider.baseUrl)
  const [status, setStatus] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const run = async (action: () => Promise<string | null>) => {
    setBusy(true)
    try {
      setStatus(await action())
      await onChanged()
    } catch (e) {
      setStatus(describe(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="provider" data-default={provider.isDefault}>
      <div className="provider__head">
        <span className="provider__label">{provider.label}</span>
        {provider.isDefault && <span className="provider__badge">по умолчанию</span>}
        {!provider.isDefault && provider.enabled && (
          <button
            type="button"
            className="settings__link"
            onClick={() => void run(async () => {
              await providerSetDefault(provider.id)
              return null
            })}
          >
            сделать основным
          </button>
        )}

        {/* Провайдеры без ключа (Ollama, LM Studio) включаются вручную:
            включать их автоматически нельзя — сервер может быть не запущен. */}
        {!provider.requiresKey && (
          <button
            type="button"
            className="settings__link"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                await providerSave(provider.id, { enabled: !provider.enabled })
                return provider.enabled ? 'выключен' : 'включён'
              })
            }
          >
            {provider.enabled ? 'выключить' : 'включить'}
          </button>
        )}
      </div>

      <div className="provider__row">
        <input
          className="settings__input"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          onBlur={() => {
            if (model !== provider.defaultModel) {
              void run(async () => {
                await providerSave(provider.id, { defaultModel: model })
                return 'модель сохранена'
              })
            }
          }}
          placeholder="Модель"
          spellCheck={false}
        />
      </div>

      {/* Адрес редактируется у всех: ТЗ §4 требует Custom API, а корпоративный
          прокси или локальный сервер меняют его и у обычных провайдеров. */}
      <div className="provider__row">
        <input
          className="settings__input"
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          onBlur={() => {
            if (baseUrl !== provider.baseUrl) {
              void run(async () => {
                await providerSave(provider.id, { baseUrl })
                return 'адрес сохранён'
              })
            }
          }}
          placeholder="Адрес API"
          spellCheck={false}
        />
      </div>

      {provider.requiresKey && (
        <div className="provider__row">
          <input
            className="settings__input"
            type="password"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder={provider.hasKey ? 'Ключ задан — введите новый, чтобы заменить' : 'API-ключ'}
            spellCheck={false}
            autoComplete="off"
          />
          <button
            type="button"
            className="settings__button"
            disabled={busy || key.trim().length === 0}
            onClick={() =>
              void run(async () => {
                await providerSetKey(provider.id, key)
                setKey('')
                return 'ключ сохранён'
              })
            }
          >
            Сохранить
          </button>
          {provider.hasKey && (
            <button
              type="button"
              className="settings__button"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  await providerClearKey(provider.id)
                  return 'ключ удалён'
                })
              }
            >
              Удалить
            </button>
          )}
        </div>
      )}

      <div className="provider__row">
        <button
          type="button"
          className="settings__button"
          disabled={busy}
          onClick={() =>
            void run(async () => {
              const models = await providerTest(provider.id)
              // Показываем не «успех», а то, что реально вернул провайдер:
              // подтверждение соединения — это список моделей, а не зелёная галочка.
              return models.length > 0
                ? `подключение работает · моделей: ${models.length}`
                : 'подключение работает, но список моделей пуст'
            })
          }
        >
          Проверить подключение
        </button>
        {status && <span className="provider__status">{status}</span>}
      </div>
    </div>
  )
}

// ── Разрешения (ТЗ §21) ─────────────────────────────────────────────────────────

function Permissions() {
  const [rows, setRows] = useState<PermissionStatus[]>([])
  const [hints, setHints] = useState<Record<string, string>>({})

  const reload = useCallback(async () => {
    const list = await permissionsList()
    setRows(list)

    // Подсказка нужна только там, где разрешение выдаёт ОС и его нет.
    const missing = list.filter((r) => !r.osGranted)
    const entries = await Promise.all(
      missing.map(async (r) => [r.category, await permissionHint(r.category)] as const),
    )
    setHints(Object.fromEntries(entries.filter(([, hint]) => hint) as [string, string][]))
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  return (
    <section className="settings__section">
      <header className="settings__header">
        <h2 className="settings__title">Разрешения</h2>
        <p className="settings__hint">
          Yuki не выполнит действие, если категория выключена здесь или не выдана
          операционной системой.
        </p>
      </header>

      <div className="settings__list">
        {rows.map((row) => (
          <label className="permission" key={row.category}>
            <input
              type="checkbox"
              checked={row.granted}
              onChange={(e) => {
                void permissionSet(row.category, e.target.checked).then(reload)
              }}
            />
            <span className="permission__name">
              {PERMISSION_LABEL[row.category] ?? row.category}
            </span>
            {!row.osGranted && (
              <span className="permission__os">
                {hints[row.category] ?? 'не выдано системой'}
              </span>
            )}
          </label>
        ))}
      </div>
    </section>
  )
}

function describe(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return 'Неизвестная ошибка'
}
