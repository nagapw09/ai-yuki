import { Fragment, useMemo } from 'react'
import type { ReactNode } from 'react'

import './Markdown.css'

/**
 * Разметка ответов модели (ТЗ §15).
 *
 * Своя реализация вместо библиотеки — сознательно. Ответ ассистента приходит из
 * сети, и любой полноценный markdown-рендерер расширяет поверхность атаки: почти
 * все они умеют встраивать HTML, а любая ссылка или картинка — это исходящий
 * запрос, который CSP окна запрещает. Поддерживается ровно то, что перечислено
 * в ТЗ §15 и что можно построить из текстовых узлов React, где XSS невозможен
 * структурно.
 *
 * Поддерживаются: заголовки, абзацы, списки, блоки кода с указанием языка,
 * `инлайн-код`, **жирный** и *курсив*.
 */

type Block =
  | { kind: 'paragraph'; lines: string[] }
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'list'; ordered: boolean; items: string[] }
  | { kind: 'code'; language: string; code: string }

const FENCE = /^```(\w*)\s*$/
const HEADING = /^(#{1,4})\s+(.*)$/
const BULLET = /^[-*]\s+(.*)$/
const ORDERED = /^\d+[.)]\s+(.*)$/

function parse(source: string): Block[] {
  const blocks: Block[] = []
  const lines = source.split('\n')
  let index = 0

  while (index < lines.length) {
    const line = lines[index] ?? ''

    const fence = FENCE.exec(line)
    if (fence) {
      const language = fence[1] ?? ''
      const code: string[] = []
      index += 1
      // Незакрытый блок — обычное дело при стриме: показываем то, что уже пришло.
      while (index < lines.length && !FENCE.test(lines[index] ?? '')) {
        code.push(lines[index] ?? '')
        index += 1
      }
      index += 1
      blocks.push({ kind: 'code', language, code: code.join('\n') })
      continue
    }

    const heading = HEADING.exec(line)
    if (heading) {
      blocks.push({
        kind: 'heading',
        level: (heading[1] ?? '#').length,
        text: heading[2] ?? '',
      })
      index += 1
      continue
    }

    const bullet = BULLET.exec(line)
    const ordered = ORDERED.exec(line)
    if (bullet || ordered) {
      const isOrdered = Boolean(ordered)
      const items: string[] = []
      while (index < lines.length) {
        const current = lines[index] ?? ''
        const match = isOrdered ? ORDERED.exec(current) : BULLET.exec(current)
        if (!match) break
        items.push(match[1] ?? '')
        index += 1
      }
      blocks.push({ kind: 'list', ordered: isOrdered, items })
      continue
    }

    if (line.trim() === '') {
      index += 1
      continue
    }

    const paragraph: string[] = []
    while (index < lines.length) {
      const current = lines[index] ?? ''
      if (
        current.trim() === '' ||
        FENCE.test(current) ||
        HEADING.test(current) ||
        BULLET.test(current) ||
        ORDERED.test(current)
      ) {
        break
      }
      paragraph.push(current)
      index += 1
    }
    blocks.push({ kind: 'paragraph', lines: paragraph })
  }

  return blocks
}

/** Разбирает инлайн-разметку в текстовые узлы React. */
function inline(text: string): ReactNode[] {
  const nodes: ReactNode[] = []
  // Порядок важен: код первым, чтобы звёздочки внутри него не съел курсив.
  const pattern = /(`[^`]+`|\*\*[^*]+\*\*|\*[^*]+\*)/g
  let last = 0
  let match: RegExpExecArray | null
  let key = 0

  while ((match = pattern.exec(text)) !== null) {
    if (match.index > last) nodes.push(text.slice(last, match.index))
    const token = match[0]

    if (token.startsWith('`')) {
      nodes.push(<code key={key++}>{token.slice(1, -1)}</code>)
    } else if (token.startsWith('**')) {
      nodes.push(<strong key={key++}>{token.slice(2, -2)}</strong>)
    } else {
      nodes.push(<em key={key++}>{token.slice(1, -1)}</em>)
    }
    last = match.index + token.length
  }

  if (last < text.length) nodes.push(text.slice(last))
  return nodes
}

export function Markdown({ source }: { source: string }) {
  const blocks = useMemo(() => parse(source), [source])

  return (
    <div className="markdown" data-selectable>
      {blocks.map((block, i) => {
        switch (block.kind) {
          case 'code':
            return (
              <pre className="markdown__code" key={i}>
                {block.language && (
                  <span className="markdown__lang">{block.language}</span>
                )}
                <code>{block.code}</code>
              </pre>
            )

          case 'heading': {
            const Tag = `h${Math.min(block.level + 1, 6)}` as 'h2'
            return <Tag key={i}>{inline(block.text)}</Tag>
          }

          case 'list':
            return block.ordered ? (
              <ol key={i}>
                {block.items.map((item, j) => (
                  <li key={j}>{inline(item)}</li>
                ))}
              </ol>
            ) : (
              <ul key={i}>
                {block.items.map((item, j) => (
                  <li key={j}>{inline(item)}</li>
                ))}
              </ul>
            )

          default:
            return (
              <p key={i}>
                {block.lines.map((line, j) => (
                  <Fragment key={j}>
                    {j > 0 && <br />}
                    {inline(line)}
                  </Fragment>
                ))}
              </p>
            )
        }
      })}
    </div>
  )
}
