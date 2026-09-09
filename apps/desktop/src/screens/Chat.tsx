import { useEffect, useRef } from 'react'

import { sendMessage } from '../agent/session'
import { CommandBar } from '../design-system/components/CommandBar'
import { Markdown } from '../design-system/components/Markdown'
import { useChatStore } from '../state/chatStore'
import type { ToolStatus } from '../state/chatStore'
import './Chat.css'

/**
 * Чат — вторичный режим, а не главный экран (ТЗ §15).
 *
 * Показываются только безопасные статусы инструментов: «Ищу файл…», «Открываю
 * браузер…». Внутренняя цепочка рассуждений сюда не попадает и не может попасть:
 * блоки `thinking` живут в истории, но в ленту не переносятся.
 */
export function Chat() {
  const entries = useChatStore((s) => s.entries)
  const streaming = useChatStore((s) => s.streaming)
  const running = useChatStore((s) => s.running)
  const error = useChatStore((s) => s.error)

  const bottom = useRef<HTMLDivElement>(null)

  useEffect(() => {
    bottom.current?.scrollIntoView({ behavior: 'smooth', block: 'end' })
  }, [entries, streaming])

  return (
    <div className="chat">
      <div className="chat__feed">
        {entries.length === 0 && !running && (
          <p className="chat__empty">
            Спроси что угодно или попроси что-нибудь сделать — Yuki покажет каждый шаг.
          </p>
        )}

        {entries.map((entry) => (
          <article className="chat__entry" data-role={entry.role} key={entry.id}>
            {entry.tools.length > 0 && (
              <div className="chat__tools">
                {entry.tools.map((tool) => (
                  <ToolChip key={tool.id} status={tool} />
                ))}
              </div>
            )}
            {entry.text &&
              (entry.role === 'assistant' ? (
                <Markdown source={entry.text} />
              ) : (
                <p className="chat__user-text" data-selectable>
                  {entry.text}
                </p>
              ))}
          </article>
        ))}

        {streaming && (
          <article className="chat__entry" data-role="assistant">
            <Markdown source={streaming} />
          </article>
        )}

        {running && !streaming && (
          <div className="chat__thinking" aria-live="polite">
            <span className="chat__dot" />
            <span className="chat__dot" />
            <span className="chat__dot" />
          </div>
        )}

        {error && (
          <p className="chat__error" role="alert" data-selectable>
            {error}
          </p>
        )}

        <div ref={bottom} />
      </div>

      <div className="chat__composer">
        <CommandBar
          onSubmit={(text) => void sendMessage(text)}
          onToggleVoice={() => undefined}
          listening={false}
          disabled={running}
        />
      </div>
    </div>
  )
}

const TOOL_STATE_LABEL: Record<ToolStatus['state'], string> = {
  running: 'выполняется',
  ok: 'готово',
  error: 'ошибка',
  blocked: 'не разрешено',
}

function ToolChip({ status }: { status: ToolStatus }) {
  // Подробности показываем только когда они что-то объясняют: для успешного
  // вызова важен факт, а не JSON результата.
  const detail = status.state === 'ok' ? undefined : status.detail

  return (
    <span className="tool-chip" data-state={status.state}>
      <span className="tool-chip__dot" aria-hidden="true" />
      <span className="tool-chip__label">{status.label}</span>
      <span className="tool-chip__state">
        {detail ?? TOOL_STATE_LABEL[status.state]}
        {status.durationMs !== undefined && status.state === 'ok'
          ? ` · ${formatDuration(status.durationMs)}`
          : ''}
      </span>
    </span>
  )
}

function formatDuration(ms: number): string {
  return ms < 1000 ? `${ms} мс` : `${(ms / 1000).toFixed(1)} с`
}
