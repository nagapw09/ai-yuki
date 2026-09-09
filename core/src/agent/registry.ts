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

  /**
   * Снимает инструмент с регистрации.
   *
   * Нужен для возможностей, которые приходят и уходят вместе с подключением:
   * инструмент выключенного MCP-сервера обязан исчезнуть из списка, который
   * видит модель, иначе она будет предлагать то, чего уже нет.
   */
  unregister(id: string): boolean {
    return this.#tools.delete(id)
  }

  /**
   * Заменяет весь набор инструментов с общим префиксом.
   *
   * Разом, а не по одному: список инструментов сервера меняется целиком, и
   * сравнивать его поэлементно значило бы оставлять расхождения при каждой
   * пропущенной ветке.
   */
  replacePrefixed(prefix: string, tools: readonly Tool[]): void {
    for (const id of [...this.#tools.keys()]) {
      if (id.startsWith(prefix)) this.#tools.delete(id)
    }
    for (const tool of tools) this.#tools.set(tool.id, tool)
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
