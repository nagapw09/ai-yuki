import { useCallback, useEffect, useState } from 'react'

import { startVoice, stopVoice } from '../agent/voice'
import { getTheme, setTheme, type ThemeMode } from '../design-system/theme'
import { useUiStore } from '../state/store'
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog'

import {
  avatarClose,
  backupExport,
  backupImport,
  backupPreview,
  backgroundSetAlwaysOnTop,
  backgroundSetAutostart,
  backgroundSetCloseToTray,
  backgroundStatus,
  onboardingReset,
  avatarOpen,
  avatarAnimations,
  avatarPlay,
  personaName,
  personaSetName,
  profileApply,
  profileDelete,
  profileList,
  profileSave,
  telegramClearToken,
  telegramPair,
  telegramRevoke,
  telegramRevokeAll,
  telegramSetToken,
  telegramStart,
  telegramStatus,
  telegramStop,
  avatarSetAlwaysOnTop,
  avatarSetAnchor,
  avatarSetAnimations,
  avatarSetClickThrough,
  avatarSetPose,
  avatarSetModel,
  avatarStatus,
  calendarAccounts,
  calendarConnect,
  calendarDisconnect,
  calendarSetClient,
  diagnosticsSave,
  hotkeyGet,
  hotkeySet,
  openUrl,
  permissionHint,
  permissionSet,
  permissionsList,
  personaGet,
  personaSet,
  privacySetLocalOnly,
  privacyStatus,
  providerClearKey,
  providerList,
  providerSave,
  providerSetDefault,
  providerSetKey,
  providerTest,
  ratesGet,
  settingGet,
  settingSet,
  systemRequirements,
  updateCheck,
  updateInstall,
  updateStatus,
  voiceConfigureStt,
  voiceSetVoice,
  voiceSpeak,
  voiceStatus,
  wakeEnrollFinish,
  wakeEnrollRecord,
  wakeForget,
  wakeStatus,
  type AnimationClip,
  type AvatarAnchor,
  type Profile,
  type TelegramStatus,
  type AvatarPose,
  type AvatarStatus,
  type BackgroundStatus,
  type CalendarAccount,
  type RequirementsReport,
  type PermissionStatus,
  type AvailableUpdate,
  type BackupSummary,
  type Persona,
  type Rates,
  type PrivacyStatus,
  type ProviderRecord,
  type WakeStatus,
  type UpdateStatus,
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
        <Character />
        <Appearance />
        <PersonaSection />
        <Everyday />
        <Privacy />
        <Remote />
        <Providers />
        <Voice />
        <Speech />
        <WakeWord />
        <Calendar />
        <Avatar />
        <Hotkey />
        <Background />
        <Permissions />
        <SystemSection />
        <Updates />
        <DataTransfer />
      </div>
    </div>
  )
}

// ── Персонаж (ТЗ §11) ───────────────────────────────────────────────────────────

/**
 * Имя ассистента и профили облика.
 *
 * Модель, анимации, характер, обращение и слово пробуждения — это не пять
 * независимых настроек, а один облик. Пока они лежали отдельно, смена
 * персонажа складывалась из пяти походов в разные части этой страницы, и
 * половина забывалась: новая модель говорила прежним голосом и откликалась на
 * прежнее имя.
 *
 * Профиль — снимок настроек целиком, а не список ссылок на них: иначе профиль,
 * сделанный полгода назад, менялся бы сам вслед за появлением новых настроек.
 */
function Character() {
  const storedName = useUiStore((s) => s.assistantName)
  const setAssistantName = useUiStore((s) => s.setAssistantName)

  const [name, setName] = useState(storedName)
  const [profiles, setProfiles] = useState<Profile[]>([])
  const [title, setTitle] = useState('')
  const [note, setNote] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    setName(storedName)
  }, [storedName])

  useEffect(() => {
    void profileList()
      .then(setProfiles)
      .catch((e: unknown) => setNote(describe(e)))
  }, [])

  const run = (action: () => Promise<string | null>) => {
    setBusy(true)
    action()
      .then(setNote)
      .catch((e: unknown) => setNote(describe(e)))
      .finally(() => setBusy(false))
  }

  const active = profiles.find((profile) => profile.active)

  return (
    <section className="settings__section">
      <h3 className="settings__title">Персонаж</h3>
      <p className="settings__hint">
        Имя видно в строке заголовка и на главном экране. Слово пробуждения при
        переименовании не меняется само: его надо переобучить в разделе
        «Обращение», иначе Yuki под новым именем продолжит откликаться на старое.
      </p>

      <div className="provider__row">
        <input
          className="settings__input settings__input--narrow"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Yuki"
          maxLength={32}
          spellCheck={false}
        />
        <button
          type="button"
          className="settings__button"
          disabled={busy}
          onClick={() =>
            run(async () => {
              await personaSetName(name)
              setAssistantName(name.trim())
              return name.trim() === '' ? 'имя как в поставке' : 'имя сохранено'
            })
          }
        >
          Переименовать
        </button>
      </div>

      <p className="settings__hint" style={{ marginTop: 'var(--space-4)' }}>
        Профиль запоминает облик целиком: модель, папку с анимациями, кадр,
        характер, обращение, имя и слово пробуждения. Переключение возвращает
        всё это одним щелчком. Положение окна и тема интерфейса в профиль не
        входят — это про рабочее место, а не про персонажа.
      </p>

      <div className="provider__row">
        <input
          className="settings__input settings__input--narrow"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder={active ? active.name : 'Название профиля'}
          spellCheck={false}
        />
        <button
          type="button"
          className="settings__button"
          disabled={busy || (title.trim() === '' && !active)}
          onClick={() =>
            run(async () => {
              const target = title.trim() || active?.name || ''
              setProfiles(await profileSave(target))
              setTitle('')
              return `сохранён: ${target}`
            })
          }
        >
          {title.trim() === '' && active ? `Обновить «${active.name}»` : 'Сохранить профиль'}
        </button>
        {note && <span className="provider__status">{note}</span>}
      </div>

      {profiles.length === 0 ? (
        <p className="settings__hint">
          Профилей пока нет. Настройте облик как нравится и сохраните — тогда к
          нему можно будет вернуться после любых опытов.
        </p>
      ) : (
        <div className="settings__list">
          {profiles.map((profile) => (
            <div className="provider__row" key={profile.id}>
              <span className="settings__hint" style={{ minWidth: '14ch' }}>
                {profile.name}
                {profile.active && ' · сейчас'}
              </span>

              {/* Профиль без модели — это характер и голос без облика; сказать
                  об этом честнее, чем показать пустое окно после применения. */}
              {!profile.hasModel && (
                <span className="provider__status">без модели</span>
              )}

              <button
                type="button"
                className="settings__button"
                disabled={busy || profile.active}
                onClick={() =>
                  run(async () => {
                    setProfiles(await profileApply(profile.id))
                    const applied = await personaName()
                    setAssistantName(applied)
                    return `применён: ${profile.name}`
                  })
                }
              >
                Применить
              </button>

              <button
                type="button"
                className="settings__button"
                disabled={busy}
                onClick={() =>
                  run(async () => {
                    setProfiles(await profileDelete(profile.id))
                    return `удалён: ${profile.name}`
                  })
                }
              >
                Удалить
              </button>
            </div>
          ))}
        </div>
      )}
    </section>
  )
}

// ── Управление с телефона (ТЗ §28, docs/REMOTE-CONTROL.md) ──────────────────────

/**
 * Канал Telegram.
 *
 * Про серверы Telegram сказано здесь, а не в документации: `docs/REMOTE-CONTROL.md`
 * §5 требует говорить об этом прямо при подключении. Человек, включающий канал,
 * должен знать, что его просьбы проходят через чужую инфраструктуру, до того как
 * включит, а не после.
 */
function Remote() {
  const [status, setStatus] = useState<TelegramStatus | null>(null)
  const [token, setToken] = useState('')
  const [fullAccess, setFullAccess] = useState(false)
  const [note, setNote] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const reload = useCallback(async () => {
    try {
      setStatus(await telegramStatus())
      setFullAccess((await settingGet('remote.full_access')) === 'on')
    } catch (e) {
      setNote(describe(e))
    }
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  // Код живёт минуты, и обратный отсчёт должен идти сам: показывать «осталось
  // 300 секунд» всё время его жизни — значит врать через минуту.
  useEffect(() => {
    if (!status?.pairingCode) return
    const timer = setInterval(() => void reload(), 1000)
    return () => clearInterval(timer)
  }, [status?.pairingCode, reload])

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

  if (!status) return null

  return (
    <section className="settings__section">
      <h3 className="settings__title">Управление с телефона</h3>
      <p className="settings__hint">
        Yuki отвечает на сообщения в Telegram: можно попросить что-нибудь из дома
        или из дороги. <strong>Сообщения проходят через серверы Telegram</strong> —
        сквозного шифрования между телефоном и этим компьютером здесь нет и быть
        не может. Поэтому канал несовместим с режимом Local Only.
      </p>

      {status.localOnly ? (
        <p className="settings__hint">
          Включён режим Local Only, и канал выключен им. Это не поломка: режим
          обещает, что наружу ничего не уходит, а Telegram — это «наружу».
        </p>
      ) : (
        <>
          <p className="settings__hint">
            Нужен свой бот: напишите <code>@BotFather</code>, команда{' '}
            <code>/newbot</code>, и вставьте сюда выданный токен. Токен ложится в
            хранилище ключей операционной системы, а не в базу.
          </p>

          <div className="provider__row">
            <input
              className="settings__input"
              type="password"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder={status.hasToken ? 'токен сохранён' : 'Токен бота'}
              spellCheck={false}
            />
            <button
              type="button"
              className="settings__button"
              disabled={busy || token.trim() === ''}
              onClick={() =>
                run(async () => {
                  const bot = await telegramSetToken(token)
                  setToken('')
                  return 'бот @' + bot + ' подключён'
                })
              }
            >
              Сохранить
            </button>
            {status.hasToken && (
              <button
                type="button"
                className="settings__button"
                disabled={busy}
                onClick={() =>
                  run(async () => {
                    await telegramClearToken()
                    return 'токен убран'
                  })
                }
              >
                Убрать токен
              </button>
            )}
          </div>

          <div className="provider__row">
            <button
              type="button"
              className="settings__button"
              data-active={status.running}
              disabled={busy || !status.hasToken}
              onClick={() =>
                run(async () => {
                  if (status.running) {
                    await telegramStop()
                    return 'канал выключен'
                  }
                  await telegramStart()
                  return 'канал работает'
                })
              }
            >
              {status.running ? 'Выключить канал' : 'Включить канал'}
            </button>

            <button
              type="button"
              className="settings__button"
              disabled={busy || !status.running}
              onClick={() =>
                run(async () => {
                  const code = await telegramPair()
                  return 'код: ' + code
                })
              }
            >
              Подключить устройство
            </button>

            {note && <span className="provider__status">{note}</span>}
          </div>

          {status.pairingCode && (
            <p className="settings__hint">
              Отправьте боту код <strong>{status.pairingCode}</strong> — он
              действует{' '}
              {status.pairingSecondsLeft !== null
                ? status.pairingSecondsLeft + ' с'
                : 'считанные минуты'}{' '}
              и только один раз. Подключение придётся подтвердить здесь, на
              компьютере.
            </p>
          )}

          <label className="hub__checkbox">
            <input
              type="checkbox"
              checked={fullAccess}
              disabled={busy}
              onChange={(e) =>
                run(async () => {
                  await settingSet('remote.full_access', e.target.checked ? 'on' : 'off')
                  return e.target.checked
                    ? 'удалённо открыты все инструменты'
                    : 'удалённо доступен обычный набор'
                })
              }
            />
            открыть удалённо все инструменты — файлы, ввод, управление окнами
          </label>

          <p className="settings__hint">
            По умолчанию с телефона доступны напоминания, заметки, память,
            календарь на чтение, погода и сведения о системе. Файлы и ввод
            закрыты: удалённо человек не видит экрана и не может проверить, что
            происходит именно то, что он имел в виду. Действия высокого риска в
            любом случае ждут подтверждения здесь, а не на телефоне.
          </p>

          {status.devices.length === 0 ? (
            <p className="settings__hint">Подключённых устройств нет.</p>
          ) : (
            <div className="settings__list">
              {status.devices.map((device) => (
                <div className="provider__row" key={device.id}>
                  <span className="settings__hint" style={{ minWidth: '16ch' }}>
                    {device.name}
                  </span>
                  <span className="provider__status">
                    {device.lastSeen
                      ? 'был на связи ' + new Date(device.lastSeen * 1000).toLocaleString()
                      : 'ещё не выходил на связь'}
                  </span>
                  <button
                    type="button"
                    className="settings__button"
                    disabled={busy}
                    onClick={() =>
                      run(async () => {
                        await telegramRevoke(device.id)
                        return 'отозван: ' + device.name
                      })
                    }
                  >
                    Отозвать
                  </button>
                </div>
              ))}

              <div className="provider__row">
                {/* Отдельная кнопка на случай украденного телефона: отзывать по
                    одному в такой момент — это время, которого нет. */}
                <button
                  type="button"
                  className="settings__button"
                  disabled={busy}
                  onClick={() =>
                    run(async () => {
                      await telegramRevokeAll()
                      return 'все устройства отозваны'
                    })
                  }
                >
                  Отозвать все
                </button>
              </div>
            </div>
          )}
        </>
      )}
    </section>
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

// ── Голос Yuki (ТЗ §10) ─────────────────────────────────────────────────────────

/**
 * Чем Yuki говорит.
 *
 * Системный синтез — SAPI на Windows, AVSpeechSynthesizer на macOS — работает
 * всегда и ничего не требует, но звука не отдаёт: приложение узнаёт только
 * «говорю» или «замолчал». Из этого следует и чужой голос, и рот аватара,
 * который двигался по правдоподобному ритму слогов, а не по звуку.
 *
 * Сервис синтеза отдаёт WAV — значит, буфер наш: Yuki играет его сама, считает
 * громкость и открывает рот ровно на звуке. Голос при этом выбирает человек:
 * GPT-SoVITS клонирует его по короткому образцу.
 */
function Speech() {
  const [status, setStatus] = useState<VoiceStatusRecord | null>(null)
  const [url, setUrl] = useState('')
  const [samples, setSamples] = useState('')
  const [prompt, setPrompt] = useState('')
  const [note, setNote] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const reload = useCallback(async () => {
    try {
      setStatus(await voiceStatus())
      setUrl((await settingGet('voice.tts.url')) ?? '')
      setSamples((await settingGet('voice.tts.samples')) ?? '')
      setPrompt((await settingGet('voice.tts.prompt')) ?? '')
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

  if (!status) return null

  const http = status.engine === 'http'

  return (
    <section className="settings__section">
      <h3 className="settings__title">Голос Yuki</h3>
      <p className="settings__hint">
        Системный голос работает всегда и ничего не требует, но звука наружу не
        отдаёт: рот аватара в этом случае двигается по ритму слогов, а не по
        речи. Сервис синтеза отдаёт запись — Yuki играет её сама, и рот идёт за
        звуком. Голос задаётся образцом: GPT-SoVITS повторяет тот, что вы дадите.
      </p>

      <div className="provider__row">
        {[
          { value: 'system', label: 'Системный' },
          { value: 'http', label: 'Сервис синтеза' },
        ].map((item) => (
          <button
            key={item.value}
            type="button"
            className="settings__button"
            data-active={status.engine === item.value}
            disabled={busy}
            onClick={() =>
              run(async () => {
                await settingSet('voice.tts.engine', item.value)
                return item.value === 'http' ? 'говорит сервис' : 'говорит система'
              })
            }
          >
            {item.label}
          </button>
        ))}

        <span className="provider__status">
          {status.hasLevel ? 'lip-sync по звуку' : 'lip-sync по ритму слогов'}
        </span>
      </div>

      {http && (
        <>
          <p className="settings__hint">
            Сервис — отдельная программа: <code>GPT-SoVITS</code> поднимает API на
            порту 9880. Он не входит в поставку и требует своих моделей и,
            как правило, видеокарты. Образцы голоса — папка с файлами{' '}
            <code>.wav</code> по несколько секунд каждый; текст образца нужен,
            чтобы сервис сверил звук со словами.
          </p>

          <div className="provider__row">
            <input
              className="settings__input"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="http://127.0.0.1:9880"
              spellCheck={false}
            />
            <button
              type="button"
              className="settings__button"
              disabled={busy}
              onClick={() =>
                run(async () => {
                  await settingSet('voice.tts.url', url.trim())
                  return 'адрес сохранён'
                })
              }
            >
              Сохранить
            </button>
          </div>

          <div className="provider__row">
            <input
              className="settings__input"
              value={samples}
              onChange={(e) => setSamples(e.target.value)}
              placeholder="Папка с образцами голоса"
              spellCheck={false}
            />
            <button
              type="button"
              className="settings__button"
              disabled={busy}
              onClick={() =>
                void openDialog({ directory: true, multiple: false }).then((picked) => {
                  if (typeof picked === 'string') {
                    setSamples(picked)
                    run(async () => {
                      await settingSet('voice.tts.samples', picked)
                      return 'папка сохранена'
                    })
                  }
                })
              }
            >
              Выбрать…
            </button>
            <button
              type="button"
              className="settings__button"
              disabled={busy}
              onClick={() =>
                run(async () => {
                  await settingSet('voice.tts.samples', samples.trim())
                  return 'папка сохранена'
                })
              }
            >
              Сохранить
            </button>
          </div>

          <div className="provider__row">
            <input
              className="settings__input"
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              placeholder="Что произнесено в образце"
              spellCheck={false}
            />
            <button
              type="button"
              className="settings__button"
              disabled={busy}
              onClick={() =>
                run(async () => {
                  await settingSet('voice.tts.prompt', prompt.trim())
                  return 'текст образца сохранён'
                })
              }
            >
              Сохранить
            </button>
          </div>

          {status.voices.length === 0 ? (
            <p className="settings__hint">
              В папке нет файлов <code>.wav</code> — сказать нечем: сервис
              синтезирует по образцу.
            </p>
          ) : (
            <div className="provider__row">
              {status.voices.map((voice) => (
                <button
                  key={voice}
                  type="button"
                  className="settings__button"
                  disabled={busy}
                  onClick={() =>
                    run(async () => {
                      await voiceSetVoice(voice)
                      return `голос: ${voice}`
                    })
                  }
                >
                  {voice}
                </button>
              ))}
            </div>
          )}
        </>
      )}

      <div className="provider__row">
        <button
          type="button"
          className="settings__button"
          disabled={busy}
          onClick={() =>
            run(async () => {
              await voiceSpeak('Привет. Это проверка голоса.')
              return 'сказала'
            })
          }
        >
          Проверить голос
        </button>
        {note && <span className="provider__status">{note}</span>}
      </div>
    </section>
  )
}

// ── Слово пробуждения (ТЗ §37) ────────────────────────────────────

/**
 * Запись обращения «Юки».
 *
 * Без записанных образцов обращение ищется в уже распознанном тексте —
 * отзыв наступает после того, как человек договорил, и бюджет ТЗ §37 не
 * выдерживается. С образцами обращение узнаётся локально и по звуку,
 * а фразы без обращения вообще не уходят на распознавание.
 */
function WakeWord() {
  const [status, setStatus] = useState<WakeStatus | null>(null)
  const [note, setNote] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    wakeStatus()
      .then(setStatus)
      .catch((e: unknown) => setNote(describe(e)))
  }, [])

  if (!status) return null

  const run = (action: () => Promise<WakeStatus>, message: string) => {
    setBusy(true)
    setNote(message)
    action()
      .then((next) => {
        setStatus(next)
        setNote(null)
      })
      .catch((e: unknown) => setNote(describe(e)))
      .finally(() => setBusy(false))
  }

  return (
    <section className="settings__section">
      <h3 className="settings__title">Обращение «Юки»</h3>
      <p className="settings__hint">
        Запишите слово {status.needed} раза — и Yuki начнёт узнавать его на месте,
        без отправки звука на распознавание. Тогда она отзывается сразу, пока
        вы ещё говорите, а фразы не к ней не уходят в сеть вовсе.
      </p>
      <p className="settings__hint">
        {/* Цену способа надо назвать до записи, а не после. */}
        Узнаёт именно ваш голос: другой человек, сказавший «Юки», скорее
        всего не будет услышан — и вы сами с сильно изменившимся голосом тоже.
        В таком случае запишите обращение заново.
      </p>

      {note && <p className="settings__hint">{note}</p>}

      <div className="provider__row">
        <span className="settings__label">
          {status.enrolled
            ? 'записано'
            : status.recorded > 0
              ? `записано ${status.recorded} из ${status.needed}`
              : 'не записано'}
        </span>

        <button
          type="button"
          className="settings__button"
          disabled={busy}
          onClick={() =>
            run(wakeEnrollRecord, 'говорите «Юки»… (две секунды)')
          }
        >
          {status.recorded === 0 ? 'Записать' : 'Ещё раз'}
        </button>

        {status.recorded >= status.needed && (
          <button
            type="button"
            className="settings__button"
            disabled={busy}
            onClick={() => run(wakeEnrollFinish, 'собираю…')}
          >
            Готово
          </button>
        )}

        {status.enrolled && (
          <button
            type="button"
            className="settings__button"
            disabled={busy}
            onClick={() => run(wakeForget, 'убираю…')}
          >
            Забыть
          </button>
        )}
      </div>
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
  const [folder, setFolder] = useState('')
  const [clips, setClips] = useState<AnimationClip[]>([])
  const [note, setNote] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const reload = useCallback(async () => {
    try {
      const next = await avatarStatus()
      setStatus(next)
      setModel(next.model)
      setFolder(next.animations)
      setNote(null)
      // Список клипов перечитывается вместе со статусом: файлы в папке могли
      // появиться после того, как её выбрали.
      setClips(next.animations === '' ? [] : await avatarAnimations().catch(() => []))
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

      <p className="settings__hint" style={{ marginTop: 'var(--space-4)' }}>
        Во весь рост аватар становится не собеседником в рамке, а существом,
        которое стоит на краю экрана. «На панели задач» прижимает окно к верху
        панели — и он стоит на ней, а не висит в случайном месте.
      </p>

      <div className="provider__row">
        {POSES.map((item) => (
          <button
            key={item.value}
            type="button"
            className="settings__button"
            data-active={status.pose === item.value}
            disabled={busy}
            onClick={() =>
              run(async () => {
                await avatarSetPose(item.value)
                return null
              })
            }
          >
            {item.label}
          </button>
        ))}

        {ANCHORS.map((item) => (
          <button
            key={item.value}
            type="button"
            className="settings__button"
            data-active={status.anchor === item.value}
            disabled={busy}
            onClick={() =>
              run(async () => {
                await avatarSetAnchor(item.value)
                return null
              })
            }
          >
            {item.label}
          </button>
        ))}
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

      <h3 className="settings__title" style={{ marginTop: 'var(--space-6)' }}>
        Анимации
      </h3>
      <p className="settings__hint">
        Папка с файлами <code>.vrma</code> — формат анимаций VRM: те же кости
        гуманоида, что у модели, поэтому один танец подходит любой. Своих клипов
        в поставке нет по той же причине, что и модели: у анимаций свои лицензии.
        Пока папки нет, аватар живёт дыханием и мимикой.
      </p>

      <div className="provider__row">
        <input
          className="settings__input"
          value={folder}
          onChange={(e) => setFolder(e.target.value)}
          placeholder="Папка с файлами .vrma"
          spellCheck={false}
        />
        <button
          type="button"
          className="settings__button"
          disabled={busy}
          onClick={() =>
            void openDialog({ directory: true, multiple: false }).then((picked) => {
              if (typeof picked === 'string') {
                setFolder(picked)
                run(async () => {
                  await avatarSetAnimations(picked)
                  return 'папка сохранена'
                })
              }
            })
          }
        >
          Выбрать…
        </button>
        <button
          type="button"
          className="settings__button"
          disabled={busy}
          onClick={() =>
            run(async () => {
              await avatarSetAnimations(folder)
              return folder.trim() === '' ? 'папка убрана' : 'папка сохранена'
            })
          }
        >
          Сохранить
        </button>
      </div>

      {status.animations !== '' && clips.length === 0 && (
        <p className="settings__hint">
          В папке нет файлов <code>.vrma</code>. Клипы из Unity (<code>.anim</code>)
          и FBX сюда не подойдут: первые — формат чужого движка, вторые несут свой
          скелет, который надо переносить на гуманоида отдельной работой.
        </p>
      )}

      {clips.length > 0 && (
        <div className="provider__row">
          {clips.map((clip) => (
            <button
              key={clip.name}
              type="button"
              className="settings__button"
              disabled={busy || !status.open}
              onClick={() =>
                run(async () => {
                  await avatarPlay(clip.name)
                  return clip.name
                })
              }
            >
              {clip.name}
            </button>
          ))}
          <button
            type="button"
            className="settings__button"
            disabled={busy || !status.open}
            onClick={() =>
              run(async () => {
                await avatarPlay('')
                return 'покой'
              })
            }
          >
            Хватит
          </button>
        </div>
      )}

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

// ── Внешний вид (docs/GAPS.md §8) ────────────────────────────────────────

const POSES: { value: AvatarPose; label: string }[] = [
  { value: 'portrait', label: 'По пояс' },
  { value: 'full', label: 'Во весь рост' },
]

const ANCHORS: { value: AvatarAnchor; label: string }[] = [
  { value: 'free', label: 'Где поставлю' },
  { value: 'taskbar', label: 'На панели задач' },
]

const THEMES: { value: ThemeMode; label: string }[] = [
  { value: 'dark', label: 'Тёмная' },
  { value: 'light', label: 'Светлая' },
  { value: 'system', label: 'Как в системе' },
]

/**
 * Тема оформления.
 *
 * Тёмная — база из ТЗ §13, а не один из двух равноправных вариантов:
 * светлая существует для тех, кому тёмный интерфейс физически тяжёл.
 */
function Appearance() {
  const [theme, setThemeState] = useState<ThemeMode>('dark')

  useEffect(() => {
    getTheme()
      .then(setThemeState)
      .catch(() => undefined)
  }, [])

  return (
    <section className="settings__section">
      <h3 className="settings__title">Внешний вид</h3>
      <p className="settings__hint">
        Тёмная тема — основная: интерфейс рисовался под неё. Светлая
        переопределяет только цвета и ничего больше.
      </p>

      <div className="provider__row">
        {THEMES.map((item) => (
          <button
            key={item.value}
            type="button"
            className="settings__button"
            data-active={theme === item.value}
            onClick={() => {
              setThemeState(item.value)
              void setTheme(item.value)
            }}
          >
            {item.label}
          </button>
        ))}
      </div>
    </section>
  )
}

// ── Роль и тон (docs/GAPS.md §7) ────────────────────────────────────────

const ROLES: { value: string; label: string; hint: string }[] = [
  { value: 'assistant', label: 'Ассистент', hint: 'Как в ТЗ: делает и отчитывается' },
  { value: 'coach', label: 'Наставник', hint: 'Помогает разобраться, не решает за вас' },
  { value: 'editor', label: 'Редактор', hint: 'Правит текст, сохраняя ваш голос' },
  { value: 'developer', label: 'Дев-помощник', hint: 'Говорит кодом и командами' },
  { value: 'custom', label: 'Своя', hint: 'Сформулируйте сами' },
]

const FORMALITY = ['на «ты»', 'нейтрально', 'на «вы»']
const VERBOSITY = ['кратко', 'обычно', 'подробно']

/**
 * Роль, тон и обращение.
 *
 * Настраивается слой поверх идентичности из ТЗ §44, а не она сама:
 * «не рапортовать о невыполненном» — это не черта характера, которую можно
 * выключить ползунком.
 */
function PersonaSection() {
  const [persona, setPersona] = useState<Persona | null>(null)
  const [preview, setPreview] = useState('')
  const [note, setNote] = useState<string | null>(null)

  useEffect(() => {
    personaGet()
      .then(setPersona)
      .catch((e: unknown) => setNote(describe(e)))
  }, [])

  if (!persona) return null

  const save = (next: Persona) => {
    setPersona(next)
    personaSet(next)
      .then((composed) => {
        setPreview(composed)
        setNote(null)
      })
      .catch((e: unknown) => setNote(describe(e)))
  }

  return (
    <section className="settings__section">
      <h3 className="settings__title">Кем быть</h3>
      <p className="settings__hint">
        Настраивается то, как Yuki ведёт разговор. Её собственные правила —
        не обещать несделанное и спрашивать перед опасным — отсюда не меняются.
      </p>

      {note && <p className="settings__error">{note}</p>}

      <div className="provider__row">
        {ROLES.map((role) => (
          <button
            key={role.value}
            type="button"
            className="settings__button"
            data-active={persona.role === role.value}
            title={role.hint}
            onClick={() => save({ ...persona, role: role.value })}
          >
            {role.label}
          </button>
        ))}
      </div>

      {persona.role === 'custom' && (
        <div className="provider__row">
          <input
            className="settings__input"
            value={persona.custom}
            onChange={(e) => setPersona({ ...persona, custom: e.target.value })}
            onBlur={() => save(persona)}
            placeholder="Например: ты строгий научный редактор"
          />
        </div>
      )}

      <div className="provider__row">
        <span className="settings__label">Тон</span>
        {FORMALITY.map((label, index) => (
          <button
            key={label}
            type="button"
            className="settings__button"
            data-active={persona.formality === index}
            onClick={() => save({ ...persona, formality: index })}
          >
            {label}
          </button>
        ))}
      </div>

      <div className="provider__row">
        <span className="settings__label">Ответы</span>
        {VERBOSITY.map((label, index) => (
          <button
            key={label}
            type="button"
            className="settings__button"
            data-active={persona.verbosity === index}
            onClick={() => save({ ...persona, verbosity: index })}
          >
            {label}
          </button>
        ))}
      </div>

      <div className="provider__row">
        <input
          className="settings__input"
          value={persona.address}
          onChange={(e) => setPersona({ ...persona, address: e.target.value })}
          onBlur={() => save(persona)}
          placeholder="Как к вам обращаться"
        />
      </div>

      {preview && (
        /* Показываем ровно тот текст, который уйдёт модели: настройка
           характера вслепую — это гадание. */
        <p className="settings__hint" data-selectable>
          Добавляется к инструкции: {preview}
        </p>
      )}
    </section>
  )
}

// ── Повседневное (docs/GAPS.md §6) ──────────────────────────────────────

/**
 * Город для погоды и валюта для курсов.
 *
 * Город спрашивается, а не определяется по IP: геолокация по адресу — это
 * отправка данных о местонахождении туда, куда человек её не просил отправлять.
 */
function Everyday() {
  const [city, setCity] = useState('')
  const [base, setBase] = useState('USD')
  const [rates, setRates] = useState<Rates | null>(null)
  const [note, setNote] = useState<string | null>(null)

  useEffect(() => {
    void settingGet('everyday.city')
      .then((value) => setCity(value ?? ''))
      .catch(() => undefined)
    void settingGet('everyday.currency')
      .then((value) => setBase(value?.trim() || 'USD'))
      .catch(() => undefined)
  }, [])

  return (
    <section className="settings__section">
      <h3 className="settings__title">Повседневное</h3>
      <p className="settings__hint">
        Город нужен, чтобы погода показывалась на главном экране. По IP он
        не определяется: это было бы отправкой данных о вашем местонахождении
        без вашего ведома. Курсы — по данным Европейского центробанка,
        обновляются раз в сутки.
      </p>

      {note && <p className="settings__error">{note}</p>}

      <div className="provider__row">
        <input
          className="settings__input"
          value={city}
          onChange={(e) => setCity(e.target.value)}
          onBlur={() => void settingSet('everyday.city', city.trim())}
          placeholder="Город для погоды"
        />
        <input
          className="settings__input"
          value={base}
          onChange={(e) => setBase(e.target.value.toUpperCase())}
          onBlur={() => void settingSet('everyday.currency', base.trim().toUpperCase())}
          placeholder="Базовая валюта, например USD"
          spellCheck={false}
        />
        <button
          type="button"
          className="settings__button"
          onClick={() =>
            void ratesGet(base)
              .then((value) => {
                setRates(value)
                setNote(null)
              })
              .catch((e: unknown) => setNote(describe(e)))
          }
        >
          Курсы сейчас
        </button>
      </div>

      {rates && (
        <p className="settings__hint">
          {/* Дата обязательна: в выходные курс стоит на пятничном,
              и без неё это выглядит как зависшие данные. */}
          На {rates.date}: 1 {rates.base} ={' '}
          {rates.rates.map((rate) => rate.value.toFixed(2) + ' ' + rate.code).join(' · ')}
        </p>
      )}
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

// ── Фоновый режим (docs/GAPS.md §3) ──────────────────────────────────────

/**
 * Трей, автозапуск и оверлей.
 *
 * Голосовой ассистент, закрывающийся по крестику, слышит только тогда,
 * когда на него смотрят. Поэтому умолчание — прятаться в трей, а
 * полный выход живёт в меню трея, где его видно.
 */
function Background() {
  const [status, setStatus] = useState<BackgroundStatus | null>(null)
  const [note, setNote] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const reload = useCallback(async () => {
    try {
      setStatus(await backgroundStatus())
      setNote(null)
    } catch (e) {
      setNote(describe(e))
    }
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  const run = (action: () => Promise<unknown>) => {
    setBusy(true)
    action()
      .then(() => reload())
      .catch((e: unknown) => setNote(describe(e)))
      .finally(() => setBusy(false))
  }

  if (!status) return null

  return (
    <section className="settings__section">
      <h3 className="settings__title">Фоновый режим</h3>
      <p className="settings__hint">
        Закрытое окно не означает выключенную Yuki: вместе с ней иначе умирают
        напоминания, сочетания клавиш и голосовой режим. Совсем выйти можно
        через меню иконки в трее — там же видно, чем она сейчас занята.
      </p>

      {note && <p className="settings__error">{note}</p>}

      <div className="provider__row">
        <label className="hub__checkbox">
          <input
            type="checkbox"
            checked={status.closeToTray}
            disabled={busy}
            onChange={(e) => run(() => backgroundSetCloseToTray(e.target.checked))}
          />
          прятать в трей вместо выхода
        </label>

        <label className="hub__checkbox">
          <input
            type="checkbox"
            checked={status.autostart}
            disabled={busy}
            onChange={(e) => run(() => backgroundSetAutostart(e.target.checked))}
          />
          запускать при входе в систему
        </label>

        <label className="hub__checkbox">
          <input
            type="checkbox"
            checked={status.alwaysOnTop}
            disabled={busy}
            onChange={(e) => run(() => backgroundSetAlwaysOnTop(e.target.checked))}
          />
          держать окно поверх остальных
        </label>
      </div>
    </section>
  )
}

// ── Система и мастер (docs/GAPS.md §1, §4) ────────────────────────────────────────

/**
 * Требования и повторный прогон мастера.
 *
 * Список требований, который негде сверить, читают один раз и забывают,
 * поэтому он сравнивается с настоящей машиной здесь же.
 */
function SystemSection() {
  const [report, setReport] = useState<RequirementsReport | null>(null)
  const [note, setNote] = useState<string | null>(null)

  useEffect(() => {
    systemRequirements()
      .then(setReport)
      .catch((e: unknown) => setNote(describe(e)))
  }, [])

  return (
    <section className="settings__section">
      <h3 className="settings__title">Система</h3>

      {note && <p className="settings__error">{note}</p>}

      {report && (
        <ul className="settings__blockers">
          {report.items.map((item) => (
            <li className="settings__blocker" key={item.key} data-ok={item.ok}>
              {item.label}: {item.actual}
              {item.ok ? '' : ` — меньше минимальных ${item.required}`}
            </li>
          ))}
          {/* Отдельно от базовых: машина может вполне тянуть Yuki и
              не тянуть локальную модель — это разные утверждения. */}
          <li className="settings__blocker" data-ok={report.localOnlyOk}>
            Режим Local Only: {report.localOnlyOk ? 'памяти хватает' : 'рекомендуется 16 ГБ'}
          </li>
        </ul>
      )}

      <div className="provider__row">
        <button
          type="button"
          className="settings__button"
          onClick={() =>
            void onboardingReset()
              .then(() => setNote('мастер откроется при следующем запуске'))
              .catch((e: unknown) => setNote(describe(e)))
          }
        >
          Пройти настройку заново
        </button>
      </div>
    </section>
  )
}

// ── Обновления (docs/GAPS.md §2) ────────────────────────────────────────────

/**
 * Проверка и установка обновлений.
 *
 * Когда канал не настроен, секция говорит это прямо, а не показывает
 * кнопку, которая молча ничего не делает: обновления — это право
 * запускать код на чужой машине, и без подписи оно выключено целиком.
 */
function Updates() {
  const [status, setStatus] = useState<UpdateStatus | null>(null)
  const [found, setFound] = useState<AvailableUpdate | null>(null)
  const [note, setNote] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    updateStatus()
      .then(setStatus)
      .catch((e: unknown) => setNote(describe(e)))
  }, [])

  if (!status) return null

  const check = () => {
    setBusy(true)
    setNote(null)
    updateCheck()
      .then((update) => {
        setFound(update)
        // «Обновлений нет» — это ответ, и он должен быть виден.
        if (!update) setNote('установлена самая свежая версия')
      })
      .catch((e: unknown) => setNote(describe(e)))
      .finally(() => setBusy(false))
  }

  return (
    <section className="settings__section">
      <h3 className="settings__title">Обновления</h3>
      <p className="settings__hint">Установлена версия {status.currentVersion}.</p>

      {!status.configured && status.reason && (
        <p className="settings__error">{status.reason}</p>
      )}

      {status.configured && (
        <div className="provider__row">
          <button type="button" className="settings__button" disabled={busy} onClick={check}>
            {busy ? 'Проверяю…' : 'Проверить обновления'}
          </button>
          {note && <span className="provider__status">{note}</span>}
        </div>
      )}

      {found && (
        <div className="provider">
          <div className="provider__head">
            <span className="provider__label">Версия {found.version}</span>
            {found.publishedAt && (
              <span className="provider__status">{found.publishedAt}</span>
            )}
          </div>

          {found.notes && (
            <p className="settings__hint" data-selectable>
              {found.notes}
            </p>
          )}

          <div className="provider__row">
            <button
              type="button"
              className="settings__button"
              disabled={busy}
              onClick={() => {
                setBusy(true)
                setNote('Скачиваю… после установки Yuki перезапустится')
                updateInstall()
                  .catch((e: unknown) => {
                    setNote(describe(e))
                    setBusy(false)
                  })
              }}
            >
              Установить и перезапустить
            </button>
          </div>
        </div>
      )}
    </section>
  )
}

// ── Перенос данных и диагностика (docs/GAPS.md §11, §14) ──────────────────────────────────────────

/**
 * Экспорт, импорт и отчёт о состоянии.
 *
 * Секреты в копию не входят, и об этом сказано до экспорта, а не после:
 * перенос, после которого половина возможностей молча не работает, хуже
 * отсутствия переноса.
 */
function DataTransfer() {
  const [summary, setSummary] = useState<BackupSummary | null>(null)
  const [note, setNote] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const run = (action: () => Promise<string | null>) => {
    setBusy(true)
    action()
      .then(setNote)
      .catch((e: unknown) => setNote(describe(e)))
      .finally(() => setBusy(false))
  }

  return (
    <section className="settings__section">
      <h3 className="settings__title">Перенос и диагностика</h3>
      <p className="settings__hint">
        В копию входят настройки, команды, память, заметки, напоминания
        и список возможностей. Ключи и токены — нет: они лежат в хранилище ОС,
        и копия, которую опасно потерять, — плохая копия.
      </p>

      {note && <p className="settings__hint">{note}</p>}

      <div className="provider__row">
        <button
          type="button"
          className="settings__button"
          disabled={busy}
          onClick={() =>
            run(async () => {
              const path = await saveDialog({
                title: 'Куда сохранить копию',
                defaultPath: 'yuki-backup.json',
                filters: [{ name: 'JSON', extensions: ['json'] }],
              })
              if (!path) return null

              const result = await backupExport(path)
              setSummary(result)
              return `сохранено: ${path}`
            })
          }
        >
          Сохранить копию
        </button>

        <button
          type="button"
          className="settings__button"
          disabled={busy}
          onClick={() =>
            run(async () => {
              const path = await openDialog({
                title: 'Файл копии',
                multiple: false,
                filters: [{ name: 'JSON', extensions: ['json'] }],
              })
              if (typeof path !== 'string') return null

              // Сначала показываем, что в файле, и только потом предлагаем
              // восстановить: импорт вслепую меняет чужие данные.
              const preview = await backupPreview(path)
              setSummary(preview)
              window.sessionStorage.setItem('yuki.backup.path', path)
              return 'файл прочитан — проверьте состав и выберите режим'
            })
          }
        >
          Открыть копию
        </button>

        <button
          type="button"
          className="settings__button"
          disabled={busy}
          onClick={() =>
            run(async () => {
              const path = await saveDialog({
                title: 'Куда сохранить отчёт',
                defaultPath: 'yuki-diagnostics.md',
                filters: [{ name: 'Markdown', extensions: ['md'] }],
              })
              if (!path) return null

              await diagnosticsSave(path)
              return `отчёт сохранён: ${path}`
            })
          }
        >
          Отчёт о состоянии
        </button>
      </div>

      {summary && (
        <>
          <ul className="settings__blockers">
            {summary.counts
              .filter((count) => count.rows > 0)
              .map((count) => (
                <li className="settings__blocker" key={count.table}>
                  {count.table}: {count.rows}
                </li>
              ))}
          </ul>

          {summary.secretsToReenter.length > 0 && (
            /* Список показывается и при экспорте, и при импорте: человек
               должен знать заранее, что придётся ввести заново. */
            <>
              <p className="settings__hint">После переноса придётся ввести заново:</p>
              <ul className="settings__blockers">
                {summary.secretsToReenter.map((item) => (
                  <li className="settings__blocker" key={item} data-ok={false}>
                    {item}
                  </li>
                ))}
              </ul>
            </>
          )}

          <div className="provider__row">
            <button
              type="button"
              className="settings__button"
              disabled={busy}
              onClick={() =>
                run(async () => {
                  const path = window.sessionStorage.getItem('yuki.backup.path')
                  if (!path) return 'сначала откройте файл копии'

                  const result = await backupImport(path, 'merge')
                  setSummary(result)
                  return 'данные дополнены из копии'
                })
              }
            >
              Дополнить из копии
            </button>

            <button
              type="button"
              className="settings__button"
              disabled={busy}
              onClick={() =>
                run(async () => {
                  const path = window.sessionStorage.getItem('yuki.backup.path')
                  if (!path) return 'сначала откройте файл копии'

                  const result = await backupImport(path, 'replace')
                  setSummary(result)
                  return 'данные заменены копией'
                })
              }
            >
              Заменить всё
            </button>
          </div>
        </>
      )}
    </section>
  )
}

function describe(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return 'Неизвестная ошибка'
}
