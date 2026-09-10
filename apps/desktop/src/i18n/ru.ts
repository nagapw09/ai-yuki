import type { Dictionary } from './index'

/** Русский — эталонный словарь: остальные языки сверяются с ним по составу ключей. */
export const ru: Dictionary = {
  'app.name': 'Yuki',

  'rail.label': 'Разделы',
  'rail.home': 'Главная',
  'rail.chat': 'Чат',
  'rail.commands': 'Команды',
  'rail.memory': 'Память',
  'rail.notes': 'Заметки',
  'rail.activity': 'Активность',
  'rail.settings': 'Настройки',

  'commandBar.placeholder': 'Скажи или напиши, что сделать',
  'commandBar.label': 'Команда для Yuki',
  'commandBar.hint': 'Ctrl + Space',
  'commandBar.startVoice': 'Начать говорить',
  'commandBar.stopVoice': 'Остановить запись',
  'commandBar.send': 'Отправить',

  'orbital.greeting': 'Чем займёмся?',
  'orbital.listening': 'Слушаю…',
  'orbital.thinking': 'Думаю…',
  'orbital.working': 'Работаю…',
  'orbital.speaking': 'Отвечаю…',
  'orbital.sleeping': 'Сплю. Скажи «Юки», чтобы разбудить',
  'orbital.today': 'Сегодня',
  'orbital.tasks': 'Задачи',
  'orbital.nextEvent': 'Далее',
  'orbital.weather': 'Погода',
  'orbital.noTasks': 'нет активных',
  'orbital.noEvents': 'ничего не запланировано',
  'orbital.tasksCount': '{count} в работе',

  'status.online': 'на связи',
  'status.offline': 'не настроен провайдер',

  'screen.soon.title': 'Раздел в разработке',
  'screen.soon.body': 'Появится в ближайшей фазе — см. docs/ROADMAP.md.',
}
