import { describe, expect, it, vi } from 'vitest'

vi.mock('../bridge', () => ({}))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }))
vi.mock('./session', () => ({ sendMessage: vi.fn() }))

import { isYes, isYesToAll } from './remote'

describe('ответ на «можно выполнить?» с телефона', () => {
  it('понимает согласие', () => {
    for (const text of ['да', 'Да!', 'ок', 'давай', 'да, отправляй', '✅ Да'.replace('✅ ', ''), 'yes']) {
      expect(isYes(text), text).toBe(true)
    }
  })
  it('«да на всё» разрешает задачу целиком', () => {
    expect(isYesToAll('да на всё')).toBe(true)
    expect(isYesToAll('Да, на всю задачу')).toBe(true)
    expect(isYesToAll('да')).toBe(false)
  })

  it('всё остальное — отказ', () => {
    for (const text of ['нет', 'не надо', 'дай подумать', 'дальше', '']) {
      expect(isYes(text), text).toBe(false)
    }
  })
})
