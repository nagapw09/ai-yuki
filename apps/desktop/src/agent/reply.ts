/**
 * Что делать с текстом ответа до того, как его увидят и услышат.
 *
 * Две вещи. Метка настроения в начале ответа — единственный канал, по которому
 * модель влияет на лицо персонажа: по разбору VPet/Desktop Mate (§26, §43)
 * модель не выбирает клипы и не управляет телом, она сообщает смысл, а что
 * из этого показать, решает сам персонаж. И текст для голоса: разметка,
 * прочитанная вслух, звучит как «звёздочка звёздочка готово».
 */

/** Настроения, которые модель может сообщить, и выражение для каждого. */
const MOODS: Record<string, string> = {
  радость: 'happy',
  грусть: 'sad',
  удивление: 'surprised',
  спокойно: 'relaxed',
  смущение: 'happy',
}

const MOOD_TAG = new RegExp(`^\\s*\\[(${Object.keys(MOODS).join('|')})\\]\\s*`, 'i')

/** Отделяет метку настроения от текста ответа. */
export function parseMood(reply: string): { text: string; expression: string | null } {
  const match = MOOD_TAG.exec(reply)
  if (!match) return { text: reply, expression: null }
  return {
    text: reply.slice(match[0].length),
    expression: MOODS[match[1]!.toLowerCase()] ?? null,
  }
}

/**
 * Фильтр потока: придерживает начало ответа, пока не станет ясно, метка это
 * или текст, — иначе «[радость]» мелькала бы в чате, пока идёт ответ.
 */
export function moodStreamFilter(): (chunk: string) => string {
  let head = ''
  let decided = false
  return (chunk) => {
    if (decided) return chunk
    head += chunk
    const trimmed = head.trimStart()
    // Метка может быть только в самом начале и короче двадцати знаков.
    if (trimmed.length > 0 && !trimmed.startsWith('[')) {
      decided = true
      return head
    }
    const close = trimmed.indexOf(']')
    if (close === -1 && trimmed.length < 20) return ''
    decided = true
    return parseMood(head).text
  }
}

/** Инструкция к стилю ответа: коротко, по-человечески, с меткой настроения. */
export const REPLY_STYLE =
  'Ты голосовой помощник, и твои ответы часто звучат вслух. Отвечай коротко и понятно: ' +
  'одно-два простых предложения, без списков, заголовков, таблиц и markdown, если человек ' +
  'сам не попросил подробностей. После действия назови результат, а не пересказывай шаги. ' +
  'Если уместно, начни ответ с одной метки настроения: [радость], [грусть], [удивление], ' +
  '[смущение] или [спокойно]. Метку не объясняй — её не читают вслух, она меняет выражение лица.'

/**
 * Текст для синтеза речи.
 *
 * Разметку убираем, ссылки не зачитываем посимвольно, длинный ответ режем
 * по предложению: полминуты голоса на вопрос «который час» — это не разговор.
 */
export function speakable(text: string, limit = 320): string {
  let out = text
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/`([^`]*)`/g, '$1')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
    .replace(/\[([^\]]+)\]\((?:https?:)?[^)]*\)/g, '$1')
    .replace(/https?:\/\/\S+/g, 'ссылка')
    .replace(/^\s{0,3}#{1,6}\s+/gm, '')
    .replace(/^\s*[-*•]\s+/gm, '')
    .replace(/^\s*\d+[.)]\s+/gm, '')
    .replace(/[*_~|>]+/g, '')
    .replace(/\s+/g, ' ')
    .trim()

  if (out.length > limit) {
    const cut = out.slice(0, limit)
    const end = Math.max(cut.lastIndexOf('. '), cut.lastIndexOf('! '), cut.lastIndexOf('? '))
    out = end > 40 ? cut.slice(0, end + 1) : `${cut.trimEnd()}…`
  }
  return out
}
