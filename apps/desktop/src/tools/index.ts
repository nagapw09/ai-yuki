/**
 * Все инструменты Yuki в одном месте.
 *
 * Список нужен единый и настоящий: по нему собирается реестр агента, по нему же
 * проверяется библиотека готовых команд (`docs/GAPS.md` §9). Две копии списка
 * разошлись бы на первом же добавленном инструменте, и шаблон начал бы ссылаться
 * на то, чего нет, — а обнаружилось бы это при запуске у пользователя.
 */

import type { Tool } from '@yuki/core'

import { BUILTIN_TOOLS } from './builtin'
import { CALENDAR_TOOLS } from './calendar'
import { CAPABILITY_TOOLS } from './capabilities'
import { COMMAND_TOOLS } from './commands'
import { EVERYDAY_TOOLS } from './everyday'
import { MEMORY_TOOLS } from './memory'
import { PLUGIN_TOOLS } from './plugins'

/** Встроенные инструменты; MCP-инструменты добавляются к ним во время работы. */
export const ALL_TOOLS: readonly Tool[] = [
  ...BUILTIN_TOOLS,
  ...MEMORY_TOOLS,
  ...CAPABILITY_TOOLS,
  ...COMMAND_TOOLS,
  ...PLUGIN_TOOLS,
  ...CALENDAR_TOOLS,
  ...EVERYDAY_TOOLS,
]

/** Идентификаторы встроенных инструментов. */
export function builtinToolIds(): ReadonlySet<string> {
  return new Set(ALL_TOOLS.map((tool) => tool.id))
}
