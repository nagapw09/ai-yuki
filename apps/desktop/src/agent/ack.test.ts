import { describe, expect, it, vi } from 'vitest'

import { ackPhrase, withAck } from './ack'

describe('отклик, пока Yuki думает', () => {
  it('отвечает по смыслу просьбы', () => {
    expect(ackPhrase('открой блокнот')).toMatch(/открыва|открою/i)
    expect(ackPhrase('найди погоду в Варшаве')).toMatch(/поищу|смотрю/i)
    expect(ackPhrase('напомни в 16:00')).toMatch(/запишу|запоминаю/i)
  })

  it('быстрый ответ обходится без отклика, долгий — получает его', async () => {
    vi.useFakeTimers()
    const say = vi.fn()
    const fast = withAck(Promise.resolve('ok'), 'открой', say)
    await vi.advanceTimersByTimeAsync(2000)
    await expect(fast).resolves.toBe('ok')
    expect(say).not.toHaveBeenCalled()

    let finish: (v: string) => void = () => {}
    const slow = withAck(new Promise<string>((r) => { finish = r }), 'открой браузер', say)
    await vi.advanceTimersByTimeAsync(1300)
    expect(say).toHaveBeenCalledTimes(1)
    finish('done')
    await expect(slow).resolves.toBe('done')
    vi.useRealTimers()
  })
})
