/**
 * Встроенные инструменты Yuki (ТЗ §6, §8, §16, §35).
 *
 * Каждый инструмент объявляет три вещи, и все три обязательны: какие категории
 * разрешений ему нужны (ТЗ §21), какой у него уровень риска (ТЗ §22) и идемпотентен
 * ли он (ТЗ §33). Без них Permission Gate не сможет принять решение, а цикл — решить,
 * безопасно ли повторить действие после сбоя.
 *
 * Описания написаны для модели, а не для человека: она выбирает инструмент только
 * по ним, поэтому здесь важно назвать не только что делает инструмент, но и когда
 * его брать не надо.
 */

import type { Tool } from '@yuki/core'

import * as bridge from '../bridge'

/** Пустая схема: инструмент без аргументов. */
const NO_ARGS = { type: 'object', properties: {}, additionalProperties: false } as const

// ── Приложения и окна (ТЗ §6) ───────────────────────────────────────────────────

const openApp: Tool = {
  id: 'open_app',
  name: 'Открыть приложение',
  description:
    'Запускает приложение по имени («Chrome», «Notion») или полному пути. ' +
    'Возвращает PID запущенного процесса. Если приложение не запустилось, ' +
    'вернётся ошибка — считать его открытым в этом случае нельзя.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: {
      app: { type: 'string', description: 'Имя, bundle id или путь к приложению' },
    },
    required: ['app'],
    additionalProperties: false,
  },
  execute: (input) => bridge.openApp((input as { app: string }).app),
}

const closeApp: Tool = {
  id: 'close_app',
  name: 'Закрыть приложение',
  description:
    'Завершает приложение. По умолчанию просит закрыться штатно, чтобы оно успело ' +
    'сохранить данные. force = true убивает процесс и может потерять несохранённое — ' +
    'применять только если пользователь прямо попросил закрыть зависшее приложение.',
  permissions: [],
  risk: 'medium',
  idempotent: false,
  inputSchema: {
    type: 'object',
    properties: {
      app: { type: 'string' },
      force: { type: 'boolean', default: false },
    },
    required: ['app'],
    additionalProperties: false,
  },
  execute: (input) => {
    const { app, force } = input as { app: string; force?: boolean }
    return bridge.closeApp(app, force ?? false)
  },
}

const listWindows: Tool = {
  id: 'list_windows',
  name: 'Список окон',
  description:
    'Возвращает открытые окна с заголовками, приложением и координатами. ' +
    'Нужен, чтобы понять, что сейчас на экране, и найти id окна для переключения.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: NO_ARGS,
  execute: () => bridge.listWindows(),
}

const focusWindow: Tool = {
  id: 'focus_window',
  name: 'Переключиться на окно',
  description: 'Выводит окно на передний план по id из list_windows.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { windowId: { type: 'number' } },
    required: ['windowId'],
    additionalProperties: false,
  },
  execute: (input) => bridge.focusWindow((input as { windowId: number }).windowId),
}

const systemInfo: Tool = {
  id: 'system_info',
  name: 'Сведения о системе',
  description: 'ОС, версия, архитектура, число ядер и память. Для вопросов о компьютере.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: NO_ARGS,
  execute: () => bridge.systemInfo(),
}

const setVolume: Tool = {
  id: 'set_volume',
  name: 'Громкость',
  description: 'Ставит громкость системного вывода. Значение от 0 (тихо) до 1 (максимум).',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { level: { type: 'number', minimum: 0, maximum: 1 } },
    required: ['level'],
    additionalProperties: false,
  },
  execute: (input) => bridge.setVolume((input as { level: number }).level),
}

// ── Файлы (ТЗ §8) ───────────────────────────────────────────────────────────────

const fileSearch: Tool = {
  id: 'file_search',
  name: 'Найти файлы',
  description:
    'Ищет файлы в каталоге. Можно ограничить расширениями и подстрокой имени. ' +
    'По умолчанию сортирует от новых к старым — так находится «последний скачанный файл». ' +
    'Путь указывать полный; домашний каталог пользователя подставляется как ~.',
  permissions: ['files'],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: {
      root: { type: 'string', description: 'Каталог, с которого начинается поиск' },
      nameContains: { type: 'string' },
      extensions: {
        type: 'array',
        items: { type: 'string' },
        description: 'Расширения без точки, например ["pdf"]',
      },
      sort: { type: 'string', enum: ['name_asc', 'modified_desc', 'size_desc'] },
      limit: { type: 'number', default: 20 },
    },
    required: ['root'],
    additionalProperties: false,
  },
  execute: (input) => {
    const args = input as Partial<bridge.FileQuery> & { root: string }
    return bridge.fileSearch(
      bridge.fileQuery(args.root, {
        nameContains: args.nameContains ?? null,
        extensions: args.extensions ?? [],
        sort: args.sort ?? 'modified_desc',
        limit: args.limit ?? 20,
      }),
    )
  },
}

const fileRead: Tool = {
  id: 'file_read_text',
  name: 'Прочитать файл',
  description:
    'Читает текстовый файл в UTF-8. Для бинарных форматов (PDF, DOCX, изображения) ' +
    'не подходит — вернётся ошибка.',
  permissions: ['files'],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { path: { type: 'string' } },
    required: ['path'],
    additionalProperties: false,
  },
  execute: (input) => bridge.fileReadText((input as { path: string }).path),
}

const fileWrite: Tool = {
  id: 'file_write_text',
  name: 'Записать файл',
  description:
    'Создаёт или полностью перезаписывает текстовый файл. Существующее содержимое ' +
    'теряется — если нужно дописать, сначала прочитай файл.',
  permissions: ['files'],
  risk: 'medium',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { path: { type: 'string' }, contents: { type: 'string' } },
    required: ['path', 'contents'],
    additionalProperties: false,
  },
  execute: (input) => {
    const { path, contents } = input as { path: string; contents: string }
    return bridge.fileWriteText(path, contents)
  },
}

const fileMove: Tool = {
  id: 'file_move',
  name: 'Переместить файл',
  description: 'Перемещает или переименовывает файл. Каталог назначения создаётся сам.',
  permissions: ['files'],
  risk: 'medium',
  idempotent: false,
  inputSchema: {
    type: 'object',
    properties: { from: { type: 'string' }, to: { type: 'string' } },
    required: ['from', 'to'],
    additionalProperties: false,
  },
  execute: (input) => {
    const { from, to } = input as { from: string; to: string }
    return bridge.fileMove(from, to)
  },
}

const fileDelete: Tool = {
  id: 'file_delete',
  name: 'Удалить файл',
  description:
    'Перемещает файл в корзину. Действие требует подтверждения пользователя. ' +
    'Вызывай только когда пользователь прямо попросил удалить.',
  permissions: ['files'],
  // ТЗ §22 относит удаление к HIGH безусловно.
  risk: 'high',
  idempotent: false,
  inputSchema: {
    type: 'object',
    properties: { path: { type: 'string' } },
    required: ['path'],
    additionalProperties: false,
  },
  execute: (input) => bridge.fileDelete((input as { path: string }).path, true),
}

const fileOpen: Tool = {
  id: 'file_open',
  name: 'Открыть файл',
  description: 'Открывает файл в приложении по умолчанию для его типа.',
  permissions: ['files'],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { path: { type: 'string' } },
    required: ['path'],
    additionalProperties: false,
  },
  execute: (input) => bridge.fileOpen((input as { path: string }).path),
}

// ── Браузер (ТЗ §7) ─────────────────────────────────────────────────────────────

const openUrl: Tool = {
  id: 'open_url',
  name: 'Открыть ссылку',
  description:
    'Открывает адрес в браузере по умолчанию. Принимает только http и https. ' +
    'Чтобы потом прочитать страницу, возьми read_ui — дерево интерфейса браузера ' +
    'содержит текст и ссылки страницы. Для действий внутри страницы — кликов, ' +
    'ввода в формы, прокрутки — нужна возможность «Браузер (Playwright)» ' +
    'из Capability Hub; без неё честно скажи, что не можешь.',
  permissions: ['browser'],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { url: { type: 'string' } },
    required: ['url'],
    additionalProperties: false,
  },
  execute: (input) => bridge.openUrl((input as { url: string }).url),
}

// ── Буфер обмена и ввод (ТЗ §6, §24) ────────────────────────────────────────────

const clipboardRead: Tool = {
  id: 'clipboard_read',
  name: 'Прочитать буфер обмена',
  description:
    'Возвращает текст из буфера обмена. Нужен для просьб вида «переведи это» ' +
    'или «объясни этот код», когда пользователь только что что-то скопировал.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: NO_ARGS,
  execute: () => bridge.clipboardRead(),
}

const clipboardWrite: Tool = {
  id: 'clipboard_write',
  name: 'Положить в буфер обмена',
  description: 'Кладёт текст в буфер обмена, чтобы пользователь мог его вставить.',
  permissions: [],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: { text: { type: 'string' } },
    required: ['text'],
    additionalProperties: false,
  },
  execute: (input) => bridge.clipboardWrite((input as { text: string }).text),
}

const typeText: Tool = {
  id: 'type_text',
  name: 'Напечатать текст',
  description:
    'Печатает текст в активное окно как последовательность нажатий клавиш. ' +
    'Перед вызовом убедись, что нужное окно в фокусе: текст уйдёт туда, где курсор.',
  permissions: ['accessibility'],
  risk: 'medium',
  idempotent: false,
  inputSchema: {
    type: 'object',
    properties: { text: { type: 'string' } },
    required: ['text'],
    additionalProperties: false,
  },
  execute: (input) => bridge.typeText((input as { text: string }).text),
}

const pressKey: Tool = {
  id: 'press_key',
  name: 'Нажать клавишу',
  description:
    'Нажимает клавишу с модификаторами. Имена: a…z, 0…9, enter, tab, space, escape, ' +
    'backspace, delete, up, down, left, right, home, end, pageup, pagedown, f1…f12. ' +
    'Модификаторы: ctrl, alt, shift, meta (Cmd на macOS, Win на Windows).',
  permissions: ['accessibility'],
  risk: 'medium',
  idempotent: false,
  inputSchema: {
    type: 'object',
    properties: {
      key: { type: 'string' },
      modifiers: {
        type: 'array',
        items: { type: 'string', enum: ['ctrl', 'alt', 'shift', 'meta'] },
      },
    },
    required: ['key'],
    additionalProperties: false,
  },
  execute: (input) => {
    const { key, modifiers } = input as { key: string; modifiers?: bridge.Modifier[] }
    return bridge.pressKey(key, modifiers ?? [])
  },
}

const mouseClick: Tool = {
  id: 'mouse_click',
  name: 'Клик мышью',
  description:
    'Двигает курсор в точку экрана и нажимает кнопку. Это запасной путь: ТЗ требует ' +
    'сначала пробовать accessibility-дерево и клавиатуру, потому что координаты ' +
    'зависят от разрешения и положения окна и легко промахиваются.',
  permissions: ['accessibility'],
  risk: 'medium',
  idempotent: false,
  inputSchema: {
    type: 'object',
    properties: {
      x: { type: 'number' },
      y: { type: 'number' },
      button: { type: 'string', enum: ['left', 'right', 'middle'] },
    },
    required: ['x', 'y'],
    additionalProperties: false,
  },
  execute: async (input) => {
    const { x, y, button } = input as { x: number; y: number; button?: bridge.MouseButton }
    await bridge.mouseMove(x, y)
    await bridge.mouseClick(button ?? 'left')
    return { x, y, button: button ?? 'left' }
  },
}

const mouseScroll: Tool = {
  id: 'mouse_scroll',
  name: 'Прокрутка',
  description:
    'Прокручивает под курсором. Положительный dy — вниз, отрицательный — вверх.',
  permissions: ['accessibility'],
  risk: 'low',
  idempotent: false,
  inputSchema: {
    type: 'object',
    properties: { dx: { type: 'number', default: 0 }, dy: { type: 'number', default: 0 } },
    additionalProperties: false,
  },
  execute: (input) => {
    const { dx, dy } = input as { dx?: number; dy?: number }
    return bridge.mouseScroll(dx ?? 0, dy ?? 0)
  },
}

// ── Экран (ТЗ §6) ───────────────────────────────────────────────────────────────

const readUi: Tool = {
  id: 'read_ui',
  name: 'Прочитать интерфейс',
  description:
    'Возвращает дерево элементов активного окна: роли, подписи, значения, ' +
    'доступные действия и координаты. Это основной способ понять, что на экране, ' +
    'и найти, на что нажать — бери его прежде снимка экрана. ' +
    'Действие press означает, что элемент можно нажать; set_value — что в него ' +
    'можно ввести текст. Если у элемента нет нужного действия, используй его ' +
    'координаты и mouse_click по центру.',
  permissions: ['accessibility'],
  risk: 'low',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: {
      windowId: { type: 'number', description: 'Из list_windows; по умолчанию активное окно' },
    },
    additionalProperties: false,
  },
  execute: (input) => {
    const { windowId } = input as { windowId?: number }
    return bridge.accessibilityText(windowId)
  },
}

const screenReadText: Tool = {
  id: 'read_screen_text',
  name: 'Прочитать текст с экрана',
  description:
    'Распознаёт текст на экране и возвращает строки с их координатами. ' +
    'Порядок выбора такой: read_ui — точнее всего и даёт готовые действия; ' +
    'этот инструмент — когда текста в дереве нет вовсе (картинки, PDF, игры, ' +
    'удалённый рабочий стол); screen_capture — последний, когда важна сама ' +
    'графика. Строки весят на порядок меньше снимка. ' +
    'Для мелкого текста бери region — точность тем выше, чем меньше лишнего вокруг. ' +
    'Распознавание ошибается на похожих символах — не выдавай его результат за точную ' +
    'цитату там, где важна буква в букву.',
  permissions: ['screen_recording'],
  risk: 'medium',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: {
      displayIndex: { type: 'number' },
      region: {
        type: 'object',
        description: 'Область экрана: x, y, width, height в пикселях',
        properties: {
          x: { type: 'number' },
          y: { type: 'number' },
          width: { type: 'number' },
          height: { type: 'number' },
        },
        required: ['x', 'y', 'width', 'height'],
      },
    },
    additionalProperties: false,
  },
  execute: async (input) => {
    const options = input as { displayIndex?: number; region?: bridge.CaptureRegion }
    const lines = await bridge.screenReadText(options)

    // Пустой результат — это ответ, а не сбой, и отличать его надо: на экране
    // может и не быть текста, а может не быть языкового пакета в системе.
    if (lines.length === 0) {
      return {
        lines: [],
        note: 'текст не найден — либо его там нет, либо в системе нет нужного языкового пакета',
      }
    }

    return { lines }
  },
}

const screenCapture: Tool = {
  id: 'screen_capture',
  name: 'Снимок экрана',
  description:
    'Делает снимок монитора и показывает его тебе. Это запасной путь: сперва ' +
    'пробуй read_ui — дерево интерфейса точнее, дешевле и содержит готовые ' +
    'действия. Снимок нужен там, где важна графика: изображения, диаграммы, ' +
    'приложения без accessibility. Описывай только то, что действительно видишь. ' +
    'Снимок уменьшается до 1280 пикселей в ширину. Если надо разобрать мелкий текст, ' +
    'бери region — область в координатах экрана: она приходит в полном разрешении ' +
    'и весит меньше целого экрана. Координаты элементов есть в ответе read_ui.',
  permissions: ['screen_recording'],
  risk: 'medium',
  idempotent: true,
  inputSchema: {
    type: 'object',
    properties: {
      displayIndex: { type: 'number' },
      region: {
        type: 'object',
        description: 'Область экрана: x, y, width, height в пикселях',
        properties: {
          x: { type: 'number' },
          y: { type: 'number' },
          width: { type: 'number' },
          height: { type: 'number' },
        },
        required: ['x', 'y', 'width', 'height'],
      },
      maxWidth: {
        type: 'number',
        description: 'Ограничение ширины в пикселях; 0 — без уменьшения',
      },
    },
    additionalProperties: false,
  },
  execute: async (input, ctx) => {
    const options = input as {
      displayIndex?: number
      region?: bridge.CaptureRegion
      maxWidth?: number
    }
    const shot = await bridge.screenCapture(options)

    // Картинка уходит модели отдельным каналом, а в результат и журнал
    // попадают только размеры: PNG весит сотни килобайт, и хранить его
    // в журнале активности (ТЗ §23) незачем.
    ctx.attach({ mediaType: 'image/png', data: shot.pngBase64 })

    // Размер возвращается фактический, после кадрирования и уменьшения:
    // по нему модель пересчитывает координаты, и обещанный размер вместо реального
    // увёл бы клик мимо.
    return { width: shot.width, height: shot.height, displayIndex: shot.displayIndex }
  },
}

/** Все встроенные инструменты. */
export const BUILTIN_TOOLS: readonly Tool[] = [
  openApp,
  closeApp,
  listWindows,
  focusWindow,
  systemInfo,
  setVolume,
  fileSearch,
  fileRead,
  fileWrite,
  fileMove,
  fileDelete,
  fileOpen,
  openUrl,
  clipboardRead,
  clipboardWrite,
  typeText,
  pressKey,
  mouseClick,
  mouseScroll,
  readUi,
  screenCapture,
  screenReadText,
]
