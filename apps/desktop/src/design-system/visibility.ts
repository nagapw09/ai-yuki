/**
 * Пауза интерфейса, когда окно спрятано в трей.
 *
 * # Зачем
 *
 * Анимации Orb (ТЗ §13) — это CSS, и WebView не останавливает их у спрятанного
 * окна сам. Измеренная цена: **21 % одного ядра в трее**, то есть впустую —
 * смотреть на анимацию в этот момент некому. Ассистент, который живёт в фоне
 * (`docs/GAPS.md` §3), не имеет права столько стоить просто за то, что запущен.
 *
 * # Почему по событию из Rust, а не по `visibilitychange`
 *
 * Tauri прячет окно через системный вызов, и страница об этом не узнаёт:
 * `document.visibilityState` остаётся `visible`. Единственный, кто знает
 * правду, — та сторона, которая окно спрятала.
 */

import { listen } from '@tauri-apps/api/event'

import { isTauri } from '../bridge'

/** Событие видимости окна из Rust. */
const EVENT = 'yuki://window-visible'

function apply(visible: boolean): void {
  if (typeof document === 'undefined') return

  if (visible) {
    delete document.documentElement.dataset.paused
  } else {
    document.documentElement.dataset.paused = 'true'
  }
}

/**
 * Начинает следить за видимостью окна. Возвращает функцию отписки.
 */
export function startVisibility(): () => void {
  if (!isTauri()) return () => {}

  const pending = listen<boolean>(EVENT, (event) => apply(event.payload))

  return () => {
    void pending.then((unlisten) => unlisten())
    // На всякий случай снимаем паузу: размонтирование не должно оставить
    // интерфейс замороженным.
    apply(true)
  }
}
