import { useCallback, useEffect, useState } from 'react'

import {
  capabilityList,
  capabilityRemove,
  capabilitySetEnabled,
  integrationInstall,
  integrationsList,
  mcpAdd,
  mcpTest,
  type CapabilityRecord,
  type Integration,
} from '../bridge'
import { useUiStore } from '../state/store'
import './Capabilities.css'

/**
 * Personal Capability Hub (ТЗ §17).
 *
 * Публичного магазина нет и не будет — это продуктовое решение из ТЗ, а не
 * упрощение. Здесь три вещи: что уже установлено, что можно поставить из
 * каталога и как добавить произвольный MCP-сервер руками.
 *
 * Главное, чего экран не делает: не показывает возможность работающей, пока
 * она не ответила. Состояние приходит из настоящего подключения, а не из факта
 * записи в базу.
 */
export function Capabilities() {
  const setScreen = useUiStore((s) => s.setScreen)
  const [installed, setInstalled] = useState<CapabilityRecord[]>([])
  const [available, setAvailable] = useState<Integration[]>([])
  const [error, setError] = useState<string | null>(null)

  const reload = useCallback(async () => {
    try {
      const [caps, list] = await Promise.all([capabilityList(), integrationsList()])
      setInstalled(caps)
      setAvailable(list)
      setError(null)
    } catch (e) {
      setError(describe(e))
    }
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  const installedIds = new Set(installed.map((c) => c.id))

  return (
    <div className="hub">
      <div className="hub__inner">
        <header className="hub__header">
          <div>
            <h2 className="hub__title">Возможности</h2>
            <p className="hub__hint">
              Всё, что Yuki умеет сверх встроенного. Магазина нет — каталог идёт
              вместе с приложением, остальное добавляется вручную или по просьбе
              к самой Yuki: «добавь возможность управлять…».
            </p>
          </div>
          <button type="button" className="hub__button" onClick={() => setScreen('settings')}>
            К настройкам
          </button>
        </header>

        {error && <p className="hub__error">{error}</p>}

        <section className="hub__section">
          <h3 className="hub__section-title">Установленные</h3>

          {installed.length === 0 && (
            <p className="hub__empty">Пока ничего не подключено.</p>
          )}

          <div className="hub__list">
            {installed.map((capability) => (
              <InstalledRow key={capability.id} capability={capability} onChanged={reload} />
            ))}
          </div>
        </section>

        <section className="hub__section">
          <h3 className="hub__section-title">Доступные интеграции</h3>

          <div className="hub__list">
            {available
              .filter((integration) => !installedIds.has(integration.id))
              .map((integration) => (
                <AvailableRow
                  key={integration.id}
                  integration={integration}
                  onChanged={reload}
                />
              ))}
          </div>
        </section>

        <AddServer onChanged={reload} />
      </div>
    </div>
  )
}

const HEALTH_LABEL: Record<CapabilityRecord['health'], string> = {
  ok: 'работает',
  degraded: 'работает частично',
  failed: 'не отвечает',
  unknown: 'не проверена',
}

function InstalledRow({
  capability,
  onChanged,
}: {
  capability: CapabilityRecord
  onChanged: () => Promise<void>
}) {
  const [busy, setBusy] = useState(false)
  const [note, setNote] = useState<string | null>(null)

  const run = async (action: () => Promise<string | null>) => {
    setBusy(true)
    setNote(null)
    try {
      setNote(await action())
      await onChanged()
    } catch (e) {
      setNote(describe(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="capability" data-health={capability.health}>
      <div className="capability__head">
        <span className="capability__name">{capability.name}</span>
        <span className="capability__source">{capability.source}</span>
        <span className="capability__health">{HEALTH_LABEL[capability.health]}</span>
      </div>

      {capability.healthNote && (
        // Причина сбоя показывается целиком: «не отвечает» без причины
        // не позволяет ничего починить.
        <p className="capability__note" data-selectable>
          {capability.healthNote}
        </p>
      )}

      {capability.tools.length > 0 && (
        <p className="capability__tools">
          Инструменты: {capability.tools.join(', ')}
        </p>
      )}

      <div className="capability__actions">
        <button
          type="button"
          className="hub__link"
          disabled={busy}
          onClick={() =>
            void run(async () => {
              const tools = await mcpTest(capability.id)
              return `подключение работает · инструментов: ${tools.length}`
            })
          }
        >
          проверить
        </button>
        <button
          type="button"
          className="hub__link"
          disabled={busy}
          onClick={() =>
            void run(async () => {
              await capabilitySetEnabled(capability.id, !capability.enabled)
              return capability.enabled ? 'выключена' : 'включена'
            })
          }
        >
          {capability.enabled ? 'выключить' : 'включить'}
        </button>
        <button
          type="button"
          className="hub__link hub__link--danger"
          disabled={busy}
          onClick={() =>
            void run(async () => {
              await capabilityRemove(capability.id)
              return null
            })
          }
        >
          удалить
        </button>
        {note && <span className="capability__status">{note}</span>}
      </div>
    </div>
  )
}

function AvailableRow({
  integration,
  onChanged,
}: {
  integration: Integration
  onChanged: () => Promise<void>
}) {
  const [secret, setSecret] = useState('')
  const [busy, setBusy] = useState(false)
  const [note, setNote] = useState<string | null>(null)

  const install = () => {
    setBusy(true)
    setNote(null)
    integrationInstall(integration.id, secret || undefined)
      .then(() => {
        setSecret('')
        return onChanged()
      })
      .catch((e: unknown) => setNote(describe(e)))
      .finally(() => setBusy(false))
  }

  return (
    <div className="capability">
      <div className="capability__head">
        <span className="capability__name">{integration.label}</span>
        {integration.community && (
          // Честно отделяем то, за что отвечает сообщество: команда запуска
          // может устареть, и пользователь должен это знать до установки.
          <span className="capability__badge">сообщество</span>
        )}
      </div>

      <p className="capability__description">{integration.description}</p>

      <div className="capability__actions">
        {integration.secretEnv && (
          <input
            className="hub__input"
            type="password"
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
            placeholder={integration.secretHint ?? 'Ключ доступа'}
            spellCheck={false}
            autoComplete="off"
          />
        )}
        <button type="button" className="hub__button" disabled={busy} onClick={install}>
          {busy ? 'Проверяю…' : 'Установить'}
        </button>
        {note && <span className="capability__status">{note}</span>}
      </div>
    </div>
  )
}

/** Добавление произвольного сервера (ТЗ §19: Add Server). */
function AddServer({ onChanged }: { onChanged: () => Promise<void> }) {
  const [open, setOpen] = useState(false)
  const [id, setId] = useState('')
  const [label, setLabel] = useState('')
  const [transport, setTransport] = useState<'stdio' | 'http'>('stdio')
  const [command, setCommand] = useState('')
  const [args, setArgs] = useState('')
  const [url, setUrl] = useState('')
  const [secret, setSecret] = useState('')
  const [secretEnv, setSecretEnv] = useState('')
  const [note, setNote] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const submit = () => {
    setBusy(true)
    setNote(null)
    mcpAdd({
      id: id.trim(),
      label: label.trim() || id.trim(),
      transport,
      command: command.trim() || undefined,
      // Аргументы разделяются пробелами — так же, как их пишут в командной строке.
      args: args.trim() ? args.trim().split(/\s+/) : [],
      url: url.trim() || undefined,
      secret: secret || undefined,
      secretEnv: secretEnv.trim() || undefined,
    })
      .then(() => {
        setOpen(false)
        setSecret('')
        return onChanged()
      })
      .catch((e: unknown) => setNote(describe(e)))
      .finally(() => setBusy(false))
  }

  if (!open) {
    return (
      <section className="hub__section">
        <button type="button" className="hub__button" onClick={() => setOpen(true)}>
          Добавить MCP-сервер вручную
        </button>
      </section>
    )
  }

  return (
    <section className="hub__section">
      <h3 className="hub__section-title">Новый MCP-сервер</h3>

      <div className="hub__form">
        <input
          className="hub__input"
          value={id}
          onChange={(e) => setId(e.target.value)}
          placeholder="Идентификатор, латиницей"
          spellCheck={false}
        />
        <input
          className="hub__input"
          value={label}
          onChange={(e) => setLabel(e.target.value)}
          placeholder="Название"
        />
        <select
          className="hub__input"
          value={transport}
          onChange={(e) => setTransport(e.target.value as 'stdio' | 'http')}
        >
          <option value="stdio">stdio — локальный процесс</option>
          <option value="http">http — удалённый сервер</option>
        </select>

        {transport === 'stdio' ? (
          <>
            <input
              className="hub__input"
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              placeholder="Команда запуска, например npx"
              spellCheck={false}
            />
            <input
              className="hub__input"
              value={args}
              onChange={(e) => setArgs(e.target.value)}
              placeholder="Аргументы через пробел"
              spellCheck={false}
            />
          </>
        ) : (
          <input
            className="hub__input"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="Адрес сервера"
            spellCheck={false}
          />
        )}

        <input
          className="hub__input"
          type="password"
          value={secret}
          onChange={(e) => setSecret(e.target.value)}
          placeholder="Ключ, если нужен"
          spellCheck={false}
          autoComplete="off"
        />
        <input
          className="hub__input"
          value={secretEnv}
          onChange={(e) => setSecretEnv(e.target.value)}
          placeholder="Переменная окружения для ключа"
          spellCheck={false}
        />
      </div>

      <div className="capability__actions">
        <button
          type="button"
          className="hub__button"
          disabled={busy || id.trim() === ''}
          onClick={submit}
        >
          {busy ? 'Подключаюсь…' : 'Добавить и проверить'}
        </button>
        <button type="button" className="hub__link" onClick={() => setOpen(false)}>
          отмена
        </button>
        {note && <span className="capability__status">{note}</span>}
      </div>
    </section>
  )
}

function describe(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return 'Неизвестная ошибка'
}
