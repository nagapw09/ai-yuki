import { describe, expect, it } from 'vitest'

import { cityFromTimeZone } from './city'

describe('город из часового пояса', () => {
  it('берёт город из обычного имени', () => {
    expect(cityFromTimeZone('Europe/Moscow')).toBe('Moscow')
    expect(cityFromTimeZone('Asia/Tokyo')).toBe('Tokyo')
  })

  it('заменяет подчёркивания пробелами', () => {
    expect(cityFromTimeZone('America/New_York')).toBe('New York')
  })

  it('берёт последний отрезок трёхуровневого имени', () => {
    expect(cityFromTimeZone('America/Argentina/Buenos_Aires')).toBe('Buenos Aires')
    expect(cityFromTimeZone('America/Indiana/Indianapolis')).toBe('Indianapolis')
  })

  // Смещение — не место. Поиск погоды по «GMT+3» нашёл бы неизвестно что и
  // показал бы это как город, где человек живёт.
  it('отказывается от смещений и сокращений', () => {
    expect(cityFromTimeZone('Etc/GMT+3')).toBeNull()
    expect(cityFromTimeZone('Etc/UTC')).toBeNull()
    expect(cityFromTimeZone('UTC')).toBeNull()
    expect(cityFromTimeZone('MSK')).toBeNull()
    expect(cityFromTimeZone('GMT-8')).toBeNull()
  })

  it('переносит дефис в названии', () => {
    expect(cityFromTimeZone('America/Port-au-Prince')).toBe('Port-au-Prince')
  })

  it('молчит, когда пояса нет', () => {
    expect(cityFromTimeZone(null)).toBeNull()
    expect(cityFromTimeZone(undefined)).toBeNull()
    expect(cityFromTimeZone('')).toBeNull()
  })
})
