/**
 * Короткий отклик, пока Yuki думает.
 *
 * Модель через CLI отвечает за несколько секунд, и молчание всё это время
 * читается как «зависла». Живой человек сначала говорит «сейчас», потом делает.
 * Поэтому, если ответ не готов быстро, Yuki отзывается одной фразой, а итог
 * присылает следом. Быстрый ответ отклика не получает — двойное «сейчас / готово»
 * на «который час» раздражало бы.
 */

/** Через сколько молчания отзываться. */
export const ACK_DELAY_MS = 1200

const pick = (list: readonly string[]) => list[Math.floor(Math.random() * list.length)]!

/** Отклик по смыслу просьбы: «открываю» на «открой», «поищу» на «найди». */
export function ackPhrase(request: string): string {
  const text = request.toLowerCase()
  // Не `\b`: в JavaScript он знает только латиницу, и на русских словах не
  // срабатывает никогда.
  const has = (words: string) => new RegExp(`(^|[^а-яё])(${words})`).test(text)
  if (has('открой|запусти|включи')) return pick(['Секунду, открываю.', 'Сейчас открою.'])
  if (has('найди|поищи|посмотри|проверь|узнай')) return pick(['Сейчас поищу.', 'Минутку, смотрю.'])
  if (has('напомни|запиши|запомни|сохрани')) return pick(['Сейчас запишу.', 'Секунду, запоминаю.'])
  if (has('напиши|отправь|ответь')) return pick(['Сейчас напишу.', 'Секунду.'])
  if (/\?\s*$/.test(request.trim())) return pick(['Сейчас подумаю.', 'Хм, секунду.'])
  return pick(['Секунду.', 'Сейчас сделаю.', 'Минутку.'])
}

/**
 * Отзывается, если `work` не закончилось за [`ACK_DELAY_MS`].
 *
 * Возвращает результат `work` как есть: отклик — побочный канал и на итог не
 * влияет, а его сбой не должен ронять саму просьбу.
 */
export async function withAck<T>(
  work: Promise<T>,
  request: string,
  say: (text: string) => unknown,
): Promise<T> {
  let settled = false
  const timer = setTimeout(() => {
    if (!settled) {
      try {
        void Promise.resolve(say(ackPhrase(request))).catch(() => undefined)
      } catch {
        /* отклик необязателен */
      }
    }
  }, ACK_DELAY_MS)
  try {
    return await work
  } finally {
    settled = true
    clearTimeout(timer)
  }
}
