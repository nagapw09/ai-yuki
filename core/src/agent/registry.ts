import type { Tool, ToolSpec } from './types'

/**
 * Реестр инструментов (ТЗ §4).
 *
 * Реестр — единственный источник правды о том, что Yuki умеет. Из него же
 * строится список, который видит модель, поэтому «инструмент есть в коде, но
 * модель о нём не знает» здесь структурно невозможно.
 */
export class ToolRegistry {
  readonly #tools = new Map<string, Tool>()

  register(tool: Tool): this {
    if (this.#tools.has(tool.id)) {
      // Молча перезаписать — значит получить в проде не тот инструмент,
      // который читаешь в коде.
      throw new Error(`инструмент ${tool.id} уже зарегистрирован`)
    }
    this.#tools.set(tool.id, tool)
    return this
  }

  registerAll(tools: readonly Tool[]): this {
    for (const tool of tools) this.register(tool)
    return this
  }

  get(id: string): Tool | undefined {
    return this.#tools.get(id)
  }

  has(id: string): boolean {
    return this.#tools.has(id)
  }

  list(): Tool[] {
    return [...this.#tools.values()]
  }

  /** Описания для модели. */
  specs(): ToolSpec[] {
    return this.list().map((tool) => ({
      name: tool.id,
      description: tool.description,
      inputSchema: tool.inputSchema,
    }))
  }

  get size(): number {
    return this.#tools.size
  }
}
