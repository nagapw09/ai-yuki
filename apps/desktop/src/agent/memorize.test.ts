import { describe, expect, it } from 'vitest'

import type { ChatEntry } from '../state/chatStore'
import { buildPrompt, parseDigest, pendingEntries } from './memorize'

const entry = (id: string, role: 'user' | 'assistant', text: string): ChatEntry => ({ id, role, text, tools: [] })

describe('memorize', () => {
  const entries = [
    entry('1', 'user', 'меня зовут Алекс'),
    entry('2', 'assistant', 'Приятно познакомиться!'),
    entry('3', 'user', 'открой хром'),
    entry('4', 'assistant', ''),
  ]

  it('takes only entries after the mark and skips empty ones', () => {
    expect(pendingEntries(entries, '2').map((e) => e.id)).toEqual(['3'])
    expect(pendingEntries(entries, 'gone').map((e) => e.id)).toEqual(['1', '2', '3'])
  })

  it('parses the digest even with chatter around the JSON', () => {
    const digest = parseDigest(
      'Вот:\n{"facts":[{"key":"Имя","content":"Пользователя зовут Алекс"},{"key":"","content":"x"}],"forget":["старый браузер"],"episode":"знакомились"}',
    )
    expect(digest).toEqual({
      facts: [{ key: 'имя', content: 'Пользователя зовут Алекс' }],
      forget: ['старый браузер'],
      episode: 'знакомились',
    })
    expect(parseDigest('не json')).toBeNull()
    expect(parseDigest('{"facts":"x","episode":null}')).toEqual({ facts: [], forget: [], episode: null })
  })

  it('shows known facts so the model reuses their keys', () => {
    const prompt = buildPrompt(entries.slice(0, 3), [{ key: 'имя', content: 'Алекс' }], '24.09.2026')
    expect(prompt).toContain('- имя: Алекс')
    expect(prompt).toContain('Пользователь: меня зовут Алекс')
  })
})
