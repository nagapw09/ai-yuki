import { useCallback, useEffect, useState } from 'react'

import {
  permissionHint,
  permissionSet,
  permissionsList,
  providerClearKey,
  providerList,
  providerSave,
  providerSetDefault,
  providerSetKey,
  providerTest,
  type PermissionStatus,
  type ProviderRecord,
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
        <Providers />
        <Permissions />
      </div>
    </div>
  )
}

// ── Провайдеры (ТЗ §4) ──────────────────────────────────────────────────────────

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
