import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { isTauri } from '../../bridge'
import './ReminderNotice.css'

interface Notice { id: string; body: string; createdAt: number }
export function ReminderNotice() {
  const [items, setItems] = useState<Notice[]>([])
  const [error, setError] = useState('')
  useEffect(() => {
    if (!isTauri()) return
    let cancelled = false
    const reload = () => { void invoke<Notice[]>('reminder_notices').then(v => { if (!cancelled) setItems(v) }).catch(() => undefined) }
    const pending = listen('yuki://reminder-fired', reload)
    reload()
    // Durable inbox repairs any event missed during startup or WebView suspension.
    const timer = setInterval(reload, 15000)
    return () => { cancelled = true; clearInterval(timer); void pending.then(off => off()) }
  }, [])
  const first = items[0]
  if (!first) return null
  return <aside className="reminder-notice" role="alert" aria-live="assertive">
    <div className="reminder-notice__head"><strong>Напоминание{items.length > 1 ? ` · ${items.length}` : ''}</strong><time>{new Date(first.createdAt * 1000).toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'})}</time></div>
    <p>{first.body}</p>
    {error && <p>{error}</p>}
    <button onClick={() => { void invoke('reminder_notice_read', {id:first.id}).then(() => {setItems(v => v.filter(i => i.id !== first.id));setError('')}).catch(e => setError(String(e))) }}>Понятно</button>
  </aside>
}
