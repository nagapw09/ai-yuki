/**
 * Удалённые просьбы: приём и ответ (ТЗ §28, `docs/REMOTE-CONTROL.md`).
 *
 * # Почему это живёт в окне
 *
 * Просьбу выполняет тот же агентный цикл, что и локальную, — со теми же
 * инструментами и тем же Permission Gate. Rust принимает сообщение и проверяет,
 * что чат сопряжён; всё остальное происходит здесь. Второй путь выполнения
 * означал бы вторую реализацию разрешений, и расходиться они начали бы в первый
 * же день.
 *
 * # Что видно человеку
 *
 * Удалённая просьба попадает в тот же чат на экране, что и локальная. Это не
 * побочный эффект, а требование прозрачности: вернувшись к компьютеру, человек
 * должен увидеть, что за него просили и что было сделано, а не обнаруживать это
 * по последствиям.
 */

import {
  activityRecord,
  settingGet,
  telegramSend,
  TELEGRAM_MESSAGE_EVENT,
  TELEGRAM_PAIRING_EVENT,
} from '../bridge'
import { listen } from '@tauri-apps/api/event'

import { useChatStore } from '../state/chatStore'
import { sendMessage, type RemoteOrigin } from './session'

/** Настройка «открыть удалённо все инструменты». */
const KEY_FULL_ACCESS = 'remote.full_access'

interface IncomingMessage {
  chatId: string
  name: string
  text: string
}

interface PairingRequest {
  chatId: string
  name: string
}

/**
 * Сколько знаков ответа уходит в мессенджер.
 *
 * Telegram обрывает сообщение на 4096 байтах, а ответ агента бывает длиннее.
 * Резать самим лучше, чем получить обрыв на середине слова от сервера: здесь
 * видно, что ответ сокращён.
 */
const REPLY_LIMIT = 3500

/**
 * Подписывается на удалённый канал.
 *
 * Возвращает функцию отписки — как и остальные подписки приложения, чтобы
 * перезапуск окна не оставлял второго слушателя, отвечающего дважды.
 */
export function startRemote(): () => void {
  const pending = [
    listen<IncomingMessage>(TELEGRAM_MESSAGE_EVENT, (event) => {
      void handle(event.payload)
    }),
    listen<PairingRequest>(TELEGRAM_PAIRING_EVENT, (event) => {
      void pair(event.payload)
    }),
  ]

  return () => {
    for (const subscription of pending) {
      void subscription.then((unlisten) => unlisten())
    }
  }
}

/** Не выполняем две удалённые просьбы разом: цикл один, и он не реентерабелен. */
let busy = false

async function handle(message: IncomingMessage): Promise<void> {
  if (busy) {
    await telegramSend(
      message.chatId,
      'Сейчас занята предыдущей просьбой. Напишите ещё раз, когда закончу.',
    ).catch(() => undefined)
    return
  }

  busy = true

  try {
    const fullAccess = (await settingGet(KEY_FULL_ACCESS).catch(() => null)) === 'on'

    const origin: RemoteOrigin = {
      channel: 'telegram',
      chatId: message.chatId,
      device: message.name,
      fullAccess,
      notify: (text) => {
        void telegramSend(message.chatId, text).catch(() => undefined)
      },
    }

    // Сам факт удалённой просьбы — уже событие для журнала, до и независимо от
    // того, какие инструменты она задействует: по журналу должно быть видно,
    // что запрос пришёл извне, даже если он ничего не сделал.
    void activityRecord({
      tool: 'remote',
      target: `telegram · ${message.name}`,
      status: 'ok',
      result: message.text,
    }).catch(() => undefined)

    const reply = await sendMessage(message.text, origin)

    // `null` означает, что просьбу выполнила записанная команда, минуя модель:
    // ответа в словах у неё нет, но молчать в мессенджере нельзя.
    await telegramSend(message.chatId, reply ? trim(reply) : 'Готово.')
  } catch (error) {
    const reason = describe(error)
    await telegramSend(message.chatId, `Не получилось: ${reason}`).catch(() => undefined)
  } finally {
    busy = false
  }
}

/**
 * Спрашивает подтверждение сопряжения — у компьютера.
 *
 * `docs/REMOTE-CONTROL.md` §2: подтверждение на телефоне означало бы, что
 * первый, кто увидел код, становится владельцем.
 */
async function pair(request: PairingRequest): Promise<void> {
  const approved = await useChatStore.getState().askConfirmation({
    toolId: 'remote_pair',
    toolName: 'Подключить устройство',
    risk: 'high',
    plan:
      `${request.name} (чат ${request.chatId}) назвал код сопряжения и просит доступ ` +
      'к Yuki из Telegram. Сообщения этого канала проходят через серверы Telegram. ' +
      'Разрешить?',
  })

  // Одобрение выполняет настройки: туда же ведёт список устройств и отзыв,
  // и держать вторую точку одобрения значило бы иметь два списка.
  const { telegramApprove } = await import('../bridge')

  if (approved) {
    await telegramApprove(request.chatId, request.name).catch(() => undefined)
    await telegramSend(request.chatId, 'Готово, устройство подключено.').catch(
      () => undefined,
    )
    void activityRecord({
      tool: 'remote_pair',
      target: `telegram · ${request.name}`,
      status: 'ok',
    }).catch(() => undefined)
    return
  }

  // Отказ на телефон не уходит, и это не забывчивость: несопряжённому чату
  // Yuki не пишет вовсе — иначе `telegram_send` стал бы способом рассылать
  // сообщения кому угодно. Про то, что без подтверждения доступа не будет,
  // сказано ещё в ответе на код.
  void activityRecord({
    tool: 'remote_pair',
    target: `telegram · ${request.name}`,
    status: 'denied',
  }).catch(() => undefined)
}

function trim(text: string): string {
  return text.length > REPLY_LIMIT ? `${text.slice(0, REPLY_LIMIT)}\n\n…ответ сокращён.` : text
}

function describe(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return 'неизвестная ошибка'
}
