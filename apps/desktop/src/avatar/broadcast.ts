/**
 * Трансляция состояния в окно аватара (ТЗ §12).
 *
 * Аватар — второе окно и второй JS-контекст: общего store у них нет. Поэтому
 * главное окно рассылает состояние событием, а аватар только слушает. Обратной
 * связи нет намеренно: два источника истины про одно состояние разошлись бы
 * при первой же гонке.
 */

import { avatarBroadcast, isTauri } from '../bridge'
import { useUiStore } from '../state/store'

/**
 * Как часто отправлять изменения громкости.
 *
 * Громкость обновляется каждый кадр захвата звука — это десятки событий в
 * секунду на пустом месте. Смена состояния уходит сразу, громкость — не чаще
 * этого интервала: рот аватара всё равно двигается плавно.
 */
const LEVEL_INTERVAL_MS = 80

/**
 * Подписывается на состояние и рассылает его. Возвращает функцию отписки.
 */
export function startAvatarBroadcast(): () => void {
  if (!isTauri()) return () => {}

  let lastState = useUiStore.getState().orbState
  let lastLevelSentAt = 0

  const send = () => {
    const { orbState, audioLevel } = useUiStore.getState()
    void avatarBroadcast({ state: orbState, audioLevel }).catch(() => {
      // Аватар может быть закрыт — это штатно, а не ошибка.
    })
  }

  send()

  return useUiStore.subscribe((state) => {
    if (state.orbState !== lastState) {
      lastState = state.orbState
      lastLevelSentAt = Date.now()
      send()
      return
    }

    const now = Date.now()
    if (now - lastLevelSentAt >= LEVEL_INTERVAL_MS) {
      lastLevelSentAt = now
      send()
    }
  })
}
