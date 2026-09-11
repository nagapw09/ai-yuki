/**
 * Человеческие названия для журнала активности.
 *
 * В журнал пишется идентификатор инструмента — `file_read_text`,
 * `mcp__testsrv__echo`. На экране он и показывался, и раздел выглядел выводом
 * консоли: чтобы понять, что произошло, надо было знать внутренние имена.
 *
 * Названия не дублируются вручную: они уже есть у самих инструментов, и
 * второй список рядом с первым разошёлся бы с ним в первый же день.
 */

import { BUILTIN_TOOLS } from '../tools/builtin'

const NAMES: ReadonlyMap<string, string> = new Map(
  BUILTIN_TOOLS.map((tool) => [tool.id, tool.name]),
)

/** Записи не от инструментов: их пишет сам агент. */
const INTERNAL: Record<string, string> = {
  agent: 'Разговор с моделью',
  command: 'Команда',
}

/**
 * Название инструмента так, как его стоит прочитать человеку.
 *
 * Неизвестный идентификатор возвращается как есть. Придумывать ему название
 * по кусочкам имени нельзя: `read_ui` превратился бы в «Читать интерфейс», а
 * инструмент мог называться иначе, и подпись врала бы.
 */
export function toolLabel(id: string): string {
  const known = NAMES.get(id) ?? INTERNAL[id]
  if (known) return known

  // Инструменты MCP-серверов приходят как `mcp__сервер__инструмент`: сервер
  // чужой, названия его инструментов мы не переводим, но хотя бы показываем,
  // чей он.
  const mcp = /^mcp__(.+?)__(.+)$/.exec(id)
  if (mcp) return `${mcp[1]} · ${mcp[2]}`

  return id
}

/** Длительность так, как её читают: миллисекунды до секунды, дальше секунды. */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} мс`
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} с`
  return `${Math.round(ms / 60_000)} мин`
}

/**
 * Короткая выжимка из результата для строки журнала.
 *
 * Полный результат остаётся доступен раскрытием: он бывает в несколько
 * килобайт JSON, и класть его в строку — это ровно тот дамп лога, из-за
 * которого раздел не читался.
 */
export function resultSummary(result: string | null): string | null {
  if (!result) return null

  const trimmed = result.trim()
  if (!trimmed) return null

  // JSON сворачиваем в одну строку: переносы внутри строки таблицы ломают ритм.
  const flat = trimmed.replace(/\s+/g, ' ')
  return flat.length > 120 ? `${flat.slice(0, 119)}…` : flat
}
