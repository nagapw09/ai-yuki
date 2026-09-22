export const isCli = (kind: string) => kind === 'claude_cli' || kind === 'codex_cli'

export function CliHelp({ kind }: { kind: string }) {
  if (!isCli(kind)) return null
  const claude = kind === 'claude_cli'
  return <div className="cli-help">
    <span className="cli-help__badge">Ваша подписка · без API-ключа</span>
    <p>Если вы уже вошли в CLI, просто нажмите «Проверить вход». Иначе выполните в терминале:</p>
    <code>{claude ? 'claude auth login' : 'codex login'}</code>
    <details><summary>Установка CLI, модели и лимиты</summary>
      <p><code>{claude ? 'npm install -g @anthropic-ai/claude-code' : 'npm install -g @openai/codex'}</code></p>
      <small>{claude ? 'Модели: default, sonnet, opus, haiku или точный ID. Программные запросы расходуют доступный для них лимит плана.' : 'default — модель CLI. Можно указать точный ID модели, доступной вашему аккаунту.'} Остаток лимита смотрите в CLI. Снимки экрана требуют API-подключения.</small>
    </details>
  </div>
}
