/**
 * Голос по образцу одной кнопкой.
 *
 * На новом компьютере голос персонажа не нужно ставить руками: кнопка
 * скачивает Python, torch, OmniVoice и модель в папку Yuki и включает голос.
 * Сам образец (запись персонажа) приезжает с переносом данных или
 * кладётся в папку образцов.
 */

import { listen } from '@tauri-apps/api/event'
import { useEffect, useState } from 'react'

import {
  VOICE_INSTALL_EVENT,
  voiceInstall,
  voiceInstallStatus,
  type VoiceInstallStatus,
} from '../bridge'

interface Stage {
  stage: string
  step: number
  steps: number
  done: boolean
  error: string | null
}

interface Download {
  what: string
  downloaded: number
  total: number
}

const mb = (bytes: number) => `${Math.round(bytes / 1048576)} МБ`

export function VoiceInstaller() {
  const [status, setStatus] = useState<VoiceInstallStatus | null>(null)
  const [stage, setStage] = useState<Stage | null>(null)
  const [download, setDownload] = useState<Download | null>(null)
  const [error, setError] = useState<string | null>(null)

  const reload = () => void voiceInstallStatus().then(setStatus).catch((e) => setError(String(e)))

  useEffect(() => {
    reload()
    const off = listen<Stage | Download>(VOICE_INSTALL_EVENT, (event) => {
      const payload = event.payload
      if ('stage' in payload) {
        setStage(payload)
        setDownload(null)
        if (payload.done) {
          setError(payload.error)
          reload()
        }
      } else {
        setDownload(payload)
      }
    })
    return () => void off.then((unlisten) => unlisten())
  }, [])

  if (!status) return null
  const busy = status.busy || (stage !== null && !stage.done)

  return (
    <section className="settings__section">
      <h3 className="settings__title">Голос персонажа</h3>
      {status.installed ? (
        <p className="settings__hint">
          Установлен: OmniVoice в <code>{status.folder}</code>. Чтобы сменить
          голос, положите запись персонажа в папку образцов ниже.
        </p>
      ) : (
        <p className="settings__hint">
          Скачает и включит голос по образцу: свой Python, torch
          {status.gpu ? ' для видеокарты NVIDIA' : ' для процессора (без видеокарты NVIDIA он будет медленным)'} и
          модель OmniVoice — всего около 6 ГБ в папку Yuki. Системный Python не
          нужен и не меняется.
        </p>
      )}

      {!status.supported ? (
        <p className="settings__hint">На этой системе установка одной кнопкой пока недоступна.</p>
      ) : (
        <div className="provider__row">
          <button
            type="button"
            className="settings__button"
            disabled={busy}
            onClick={() => {
              setError(null)
              setStage({ stage: 'Начинаю', step: 0, steps: 6, done: false, error: null })
              void voiceInstall().catch((e) => {
                setError(String(e))
                setStage(null)
              })
            }}
          >
            {status.installed ? 'Переустановить голос' : 'Скачать и включить голос'}
          </button>
          {stage && (
            <span className="provider__status">
              {stage.done ? stage.stage : `${stage.step}/${stage.steps} · ${stage.stage}`}
              {download && !stage.done && download.total > 0
                ? ` · ${mb(download.downloaded)} из ${mb(download.total)}`
                : ''}
            </span>
          )}
        </div>
      )}
      {error && <p className="settings__error" role="alert">{error}</p>}
    </section>
  )
}
