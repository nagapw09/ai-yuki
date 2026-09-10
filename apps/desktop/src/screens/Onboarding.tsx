import { useCallback, useEffect, useState } from 'react'

import {
  hotkeyGet,
  onboardingFinish,
  onboardingStatus,
  permissionOpenSettings,
  permissionRequestOs,
  permissionSet,
  providerList,
  providerSave,
  providerSetDefault,
  providerSetKey,
  providerTest,
  type OnboardingStatus,
  type PermissionStep,
  type ProviderRecord,
  type RequirementsReport,
} from '../bridge'
import { useI18n, type Locale } from '../i18n'
import './Onboarding.css'

/**
 * Мастер первого запуска (`docs/GAPS.md` §1).
 *
 * # Почему он существует
 *
 * ТЗ §21 перечисляет категории разрешений, но не описывает, как их выдают.
 * На macOS без этого мастера приложение молча не работает: Accessibility и
 * Screen Recording выдаются только вручную, а до выдачи системный API
 * возвращает не ошибку, а пустоту — и Yuki уверенно рассказывает про пустой
 * экран.
 *
 * # Правило, которому подчинены все шаги
 *
 * Шаг отмечается пройденным только по проверенному факту: провайдер — по
 * ответу на Test Connection, разрешение — по опросу системы. Никаких «вы
 * наверное уже выдали».
 */
export function Onboarding({ onDone }: { onDone: () => void }) {
  const [status, setStatus] = useState<OnboardingStatus | null>(null)
  const [step, setStep] = useState(0)
  const [error, setError] = useState<string | null>(null)

  const reload = useCallback(async () => {
    try {
      setStatus(await onboardingStatus())
      setError(null)
    } catch (e) {
      setError(describe(e))
    }
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  if (!status) {
    return (
      <div className="onboarding">
        <div className="onboarding__card">
          {error ? <p className="onboarding__error">{error}</p> : <p>Минуту…</p>}
        </div>
      </div>
    )
  }

  const steps = [
    { title: 'Язык', node: <LanguageStep /> },
    { title: 'Модель', node: <ProviderStep onChanged={reload} /> },
    {
      title: 'Разрешения',
      node: <PermissionsStep permissions={status.permissions} onChanged={reload} />,
    },
    {
      title: 'Проверка',
      node: <ReadyStep status={status} />,
    },
  ]

  const finish = () => {
    void onboardingFinish()
      .then(onDone)
      .catch((e: unknown) => setError(describe(e)))
  }

  const last = step === steps.length - 1

  return (
    <div className="onboarding">
      <div className="onboarding__card">
        <header className="onboarding__header">
          <span className="onboarding__brand">YUKI</span>
          <ol className="onboarding__steps">
            {steps.map((item, index) => (
              <li
                key={item.title}
                className="onboarding__step"
                data-state={index === step ? 'current' : index < step ? 'done' : 'next'}
              >
                {item.title}
              </li>
            ))}
          </ol>
        </header>

        {error && <p className="onboarding__error">{error}</p>}

        <div className="onboarding__body">{steps[step]?.node}</div>

        <footer className="onboarding__footer">
          {step > 0 && (
            <button
              type="button"
              className="onboarding__link"
              onClick={() => setStep((s) => s - 1)}
            >
              назад
            </button>
          )}

          {/* Пропустить можно всегда и это не спрятано: мастер, из которого
              нельзя выйти, — это не забота, а шантаж. Невыданное останется
              видно в настройках. */}
          <button type="button" className="onboarding__link" onClick={finish}>
            пропустить настройку
          </button>

          <button
            type="button"
            className="onboarding__button"
            onClick={() => (last ? finish() : setStep((s) => s + 1))}
          >
            {last ? 'Начать' : 'Дальше'}
          </button>
        </footer>
      </div>
    </div>
  )
}

// ── Шаг 1: язык ─────────────────────────────────────────────────────────────────

const LANGUAGES: { value: Locale; label: string }[] = [
  { value: 'ru', label: 'Русский' },
  { value: 'en', label: 'English' },
  { value: 'uk', label: 'Українська' },
]

function LanguageStep() {
  const { locale, setLocale } = useI18n()

  return (
    <div className="onboarding__section">
      <h2 className="onboarding__title">На каком языке говорить?</h2>
      <p className="onboarding__hint">
        Язык интерфейса и распознавания речи. Понимать Yuki будет и другие —
        это выбор того, на чём она отвечает по умолчанию.
      </p>

      <div className="onboarding__choices">
        {LANGUAGES.map((language) => (
          <button
            key={language.value}
            type="button"
            className="onboarding__choice"
            data-selected={locale === language.value}
            onClick={() => setLocale(language.value)}
          >
            {language.label}
          </button>
        ))}
      </div>
    </div>
  )
}

// ── Шаг 2: провайдер ────────────────────────────────────────────────────────────

/**
 * Выбор модели.
 *
 * Проверка подключения обязательна не для галочки: без неё человек уходит из
 * мастера с ощущением, что всё настроено, и узнаёт правду на первой же реплике.
 */
function ProviderStep({ onChanged }: { onChanged: () => Promise<void> }) {
  const [providers, setProviders] = useState<ProviderRecord[]>([])
  const [chosen, setChosen] = useState<string>('')
  const [key, setKey] = useState('')
  const [note, setNote] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(async () => {
    const list = await providerList().catch(() => [])
    setProviders(list)
    setChosen((current) => current || list.find((p) => p.isDefault)?.id || list[0]?.id || '')
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  const provider = providers.find((p) => p.id === chosen)

  const connect = () => {
    if (!provider) return
    setBusy(true)
    setNote(null)

    void (async () => {
      try {
        if (provider.requiresKey && key.trim() !== '') {
          await providerSetKey(provider.id, key)
        } else if (!provider.requiresKey) {
          await providerSave(provider.id, { enabled: true })
        }

        const models = await providerTest(provider.id)
        await providerSetDefault(provider.id)
        setKey('')
        setNote(
          models.length > 0
            ? `готово · доступно моделей: ${models.length}`
            : 'подключение работает, но список моделей пуст',
        )
        await load()
        await onChanged()
      } catch (e) {
        setNote(describe(e))
      } finally {
        setBusy(false)
      }
    })()
  }

  return (
    <div className="onboarding__section">
      <h2 className="onboarding__title">Какой моделью пользоваться?</h2>
      <p className="onboarding__hint">
        Yuki не привязана к одному сервису. Облачные модели умнее, локальные —
        Ollama и LM Studio — не отправляют ни слова с компьютера. Поменять можно
        в любой момент.
      </p>

      <div className="onboarding__choices">
        {providers.map((item) => (
          <button
            key={item.id}
            type="button"
            className="onboarding__choice"
            data-selected={item.id === chosen}
            onClick={() => setChosen(item.id)}
          >
            {item.label}
            {!item.requiresKey && <span className="onboarding__badge">локально</span>}
            {item.hasKey && <span className="onboarding__badge">ключ задан</span>}
          </button>
        ))}
      </div>

      {provider?.requiresKey && (
        <input
          className="onboarding__input"
          type="password"
          value={key}
          onChange={(e) => setKey(e.target.value)}
          placeholder={provider.hasKey ? 'Ключ уже задан — введите новый, чтобы заменить' : 'API-ключ'}
          spellCheck={false}
          autoComplete="off"
        />
      )}

      <div className="onboarding__row">
        <button
          type="button"
          className="onboarding__button"
          disabled={busy || !provider}
          onClick={connect}
        >
          {busy ? 'Проверяю…' : 'Проверить подключение'}
        </button>
        {note && <span className="onboarding__note">{note}</span>}
      </div>
    </div>
  )
}

// ── Шаг 3: разрешения ───────────────────────────────────────────────────────────

const PERMISSION_LABEL: Record<string, string> = {
  files: 'Файлы',
  accessibility: 'Управление интерфейсом',
  screen_recording: 'Запись экрана',
  microphone: 'Микрофон',
  notifications: 'Уведомления',
  browser: 'Браузер',
}

const PERMISSION_WHY: Record<string, string> = {
  files: 'найти документ, открыть его, переложить в другую папку',
  accessibility: 'читать интерфейс других программ и нажимать в них кнопки',
  screen_recording: 'видеть экран, когда без картинки не разобраться',
  microphone: 'слышать вас — без него голосовой режим не работает',
  notifications: 'напоминать о встречах и сообщать о готовности',
  browser: 'открывать ссылки в вашем браузере',
}

function PermissionsStep({
  permissions,
  onChanged,
}: {
  permissions: PermissionStep[]
  onChanged: () => Promise<void>
}) {
  return (
    <div className="onboarding__section">
      <h2 className="onboarding__title">Что Yuki разрешено делать</h2>
      <p className="onboarding__hint">
        Каждое разрешение можно отозвать в настройках. Опасные действия — удаление,
        терминал, изменения в системе — всё равно спрашивают подтверждение
        отдельно, каждый раз.
      </p>

      <div className="onboarding__permissions">
        {permissions.map((permission) => (
          <PermissionRow key={permission.category} permission={permission} onChanged={onChanged} />
        ))}
      </div>
    </div>
  )
}

function PermissionRow({
  permission,
  onChanged,
}: {
  permission: PermissionStep
  onChanged: () => Promise<void>
}) {
  const [busy, setBusy] = useState(false)

  const run = (action: () => Promise<unknown>) => {
    setBusy(true)
    void action()
      .then(() => onChanged())
      .finally(() => setBusy(false))
  }

  // Разрешение работает только когда согласны обе стороны: и пользователь
  // внутри Yuki, и операционная система. Показывать «выдано» по одной из них
  // значит обещать то, чего нет.
  const ready = permission.userGranted && permission.osGranted

  return (
    <div className="onboarding__permission" data-ready={ready}>
      <label className="onboarding__permission-head">
        <input
          type="checkbox"
          checked={permission.userGranted}
          disabled={busy}
          onChange={(e) =>
            run(() => permissionSet(permission.category, e.target.checked))
          }
        />
        <span className="onboarding__permission-name">
          {PERMISSION_LABEL[permission.category] ?? permission.category}
        </span>
        {permission.required && <span className="onboarding__badge">нужно для работы</span>}
      </label>

      <p className="onboarding__permission-why">
        {PERMISSION_WHY[permission.category] ?? ''}
      </p>

      {permission.userGranted && !permission.osGranted && (
        <div className="onboarding__permission-os">
          <span className="onboarding__warn">
            {permission.hint
              ? `Система пока не пускает. Выдайте вручную: ${permission.hint}`
              : 'Система пока не пускает.'}
          </span>

          <div className="onboarding__row">
            {permission.canRequest && (
              <button
                type="button"
                className="onboarding__link"
                disabled={busy}
                onClick={() => run(() => permissionRequestOs(permission.category))}
              >
                запросить у системы
              </button>
            )}
            {permission.canOpenSettings && (
              <button
                type="button"
                className="onboarding__link"
                disabled={busy}
                onClick={() => run(() => permissionOpenSettings(permission.category))}
              >
                открыть настройки системы
              </button>
            )}
            {/* Статус спрашивается у системы заново, а не предполагается:
                человек уходит выдавать разрешение и возвращается сюда. */}
            <button
              type="button"
              className="onboarding__link"
              disabled={busy}
              onClick={() => run(async () => undefined)}
            >
              проверить ещё раз
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

// ── Шаг 4: готовность ───────────────────────────────────────────────────────────

function ReadyStep({ status }: { status: OnboardingStatus }) {
  const missing = status.permissions.filter((p) => p.required && !(p.userGranted && p.osGranted))

  // Сочетание спрашивается, а не вписывается в текст: оно разное на Windows
  // и macOS и его могли поменять. Написанное наизусть сочетание — это
  // инструкция, которая перестанет работать тихо.
  const [hotkey, setHotkey] = useState<string | null>(null)

  useEffect(() => {
    hotkeyGet()
      .then(setHotkey)
      .catch(() => undefined)
  }, [])

  return (
    <div className="onboarding__section">
      <h2 className="onboarding__title">Всё готово</h2>

      <ul className="onboarding__summary">
        <li data-ok={status.providerReady}>
          {status.providerReady
            ? `Модель: ${status.providerLabel ?? 'выбрана'}`
            : 'Модель не выбрана — Yuki не сможет ответить, пока не настроите её в настройках'}
        </li>
        <li data-ok={missing.length === 0}>
          {missing.length === 0
            ? 'Разрешения выданы'
            : `Не выдано: ${missing
                .map((p) => PERMISSION_LABEL[p.category] ?? p.category)
                .join(', ')}. Эти функции будут честно отказывать, пока разрешение не появится`}
        </li>
      </ul>

      <Requirements report={status.requirements} />

      <p className="onboarding__hint">
        Скажите «Юки, открой браузер» или напишите то же самое в строке внизу.
        {hotkey ? ` Вызов из любого места — ${hotkey}.` : ''}
      </p>
    </div>
  )
}

/** Сверка с минимальными требованиями (`docs/GAPS.md` §4). */
export function Requirements({ report }: { report: RequirementsReport }) {
  const failed = report.items.filter((item) => !item.ok)

  return (
    <div className="onboarding__requirements">
      {failed.length > 0 && (
        <>
          <p className="onboarding__warn">
            Машина слабее минимальных требований — Yuki запустится, но будет
            заметно медленнее:
          </p>
          <ul className="onboarding__summary">
            {failed.map((item) => (
              <li key={item.key} data-ok={false}>
                {item.label}: нужно {item.required}, есть {item.actual}
              </li>
            ))}
          </ul>
        </>
      )}

      {failed.length === 0 && !report.localOnlyOk && (
        // Не ошибка, а предупреждение: базовым требованиям машина отвечает,
        // а локальная модель на ней будет отвечать минутами.
        <p className="onboarding__hint">
          Памяти хватает для работы с облачной моделью. Для режима Local Only
          с локальной моделью рекомендуется 16 ГБ.
        </p>
      )}
    </div>
  )
}

function describe(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return 'Неизвестная ошибка'
}
