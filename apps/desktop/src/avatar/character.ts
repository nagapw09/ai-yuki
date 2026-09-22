/**
 * Состояние персонажа и выбор занятия (Utility AI).
 *
 * # Зачем это вместо случайного выбора
 *
 * Раньше компаньон брал следующее занятие жребием из списка. Со стороны это
 * читается как перебор картинок: он ложится спать бодрым, гуляет сразу после
 * прогулки и одинаково равнодушен к тому, что происходит вокруг.
 *
 * Здесь у него появляются простые внутренние величины — силы, скука,
 * привязанность, любопытство, — и занятие выбирается по ним. Одно и то же
 * событие при разных величинах приводит к разному поведению, и именно это
 * создаёт ощущение живого существа, а не набора анимаций.
 *
 * # Почему не языковая модель
 *
 * Повседневное поведение обязано работать всегда: без сети, без ключей, без
 * ответа модели и без задержки в секунду на каждое движение. Модель отвечает за
 * разговор и характер, а чем занять себя в тишине — решает этот код.
 */

/** Внутренние величины персонажа. Все в диапазоне 0…1. */
export interface Vitals {
  /** Силы. Тратятся бодрствованием, восстанавливаются сном. */
  energy: number
  /** Скука. Растёт от бездействия, спадает от смены занятия. */
  boredom: number
  /** Привязанность. Растёт от внимания человека, очень медленно тает. */
  affection: number
  /** Любопытство — черта характера: насколько тянет к курсору и к новому. */
  curiosity: number
}

export const DEFAULT_VITALS: Vitals = { energy: 0.8, boredom: 0.1, affection: 0.3, curiosity: 0.6 }

const clamp = (value: number) => Math.min(1, Math.max(0, value))

/** Читает сохранённые величины, не доверяя содержимому файла настроек. */
export function parseVitals(raw: string | null): Vitals {
  try {
    const parsed = JSON.parse(raw || '{}') as Partial<Record<keyof Vitals, unknown>>
    const pick = (key: keyof Vitals) => {
      const value = parsed[key]
      return typeof value === 'number' && Number.isFinite(value) ? clamp(value) : DEFAULT_VITALS[key]
    }
    return { energy: pick('energy'), boredom: pick('boredom'), affection: pick('affection'), curiosity: pick('curiosity') }
  } catch {
    return { ...DEFAULT_VITALS }
  }
}

/**
 * Насколько курсор рядом, 0…1.
 *
 * Расстояние считается в долях окна от центра персонажа: у самого тела — почти
 * единица, у края окна — ноль. Курсор вне окна ощущается как «никого рядом».
 */
export function cursorCloseness(x: number, y: number): number {
  if (!Number.isFinite(x) || !Number.isFinite(y)) return 0
  if (x < 0 || x > 1 || y < 0 || y > 1) return 0
  const distance = Math.hypot(x - 0.5, y - 0.55) * 2
  return clamp(1 - distance)
}

/** Что изменилось вокруг персонажа с прошлого шага. */
export interface Surroundings {
  /** Секунд с прошлого шага. */
  delta: number
  /** Человек не трогал мышь и клавиатуру столько секунд. */
  idleSeconds: number
  /** Насколько близко курсор, 0…1. */
  cursor: number
  /** Спит ли персонаж прямо сейчас. */
  sleeping: boolean
  /** Занят ли он чем-то, кроме покоя. */
  busy: boolean
}

/**
 * Продвигает внутренние величины во времени.
 *
 * Скорости подобраны так, чтобы полный цикл был заметен за часы работы, а не за
 * минуты: персонаж, засыпающий каждые пять минут, выглядит не живым, а больным.
 */
export function advance(vitals: Vitals, world: Surroundings): Vitals {
  const hours = Math.max(0, world.delta) / 3600

  // Сон восстанавливает силы вчетверо быстрее, чем бодрствование их тратит.
  const energy = world.sleeping ? vitals.energy + hours * 0.5 : vitals.energy - hours * 0.12

  // Скука растёт в покое и падает, пока персонаж занят. Внимание человека
  // разгоняет её сильнее всего: рядом с ним скучать не приходится.
  const boring = world.busy ? -0.9 : world.cursor > 0.4 ? -0.35 : 0.5
  const boredom = vitals.boredom + hours * boring

  // Привязанность тает много медленнее, чем растёт: её набирают неделями.
  const affection = vitals.affection - hours * 0.01

  return {
    energy: clamp(energy),
    boredom: clamp(boredom),
    affection: clamp(affection),
    curiosity: clamp(vitals.curiosity),
  }
}

/** Внимание человека: погладили, поговорили, позвали. */
export function noticed(vitals: Vitals, strength = 0.05): Vitals {
  return {
    ...vitals,
    affection: clamp(vitals.affection + strength),
    boredom: clamp(vitals.boredom - strength * 3),
  }
}

/** Что персонаж может делать прямо сейчас. */
export interface Choice {
  action: string
  /** Насколько это уместно сейчас. Ниже нуля — не предлагать вовсе. */
  score: number
}

export interface ChoiceInput {
  vitals: Vitals
  idleSeconds: number
  cursor: number
  /** Играет ли музыка и разрешено ли под неё танцевать. */
  music: boolean
  /** Есть ли место для ходьбы. */
  canWalk: boolean
  /** Какое занятие идёт сейчас: повторять его подряд незачем. */
  current: string
  available: (action: string) => boolean
}

/**
 * Оценивает занятия и возвращает их по убыванию уместности.
 *
 * Возвращается весь список, а не победитель: вызывающий добавляет случайность
 * между близкими вариантами, иначе при одних и тех же величинах персонаж будет
 * делать одно и то же.
 */
export function rank(input: ChoiceInput): Choice[] {
  const { vitals: v, cursor, idleSeconds } = input
  const away = idleSeconds > 180
  const longAway = idleSeconds > 600

  const scores: Record<string, number> = {
    // Спать хочется тем сильнее, чем меньше сил; в отсутствие человека — раньше.
    'builtin:sleep': (1 - v.energy) * 0.8 + (longAway ? 0.5 : 0),
    // Курсор рядом — повод обратить на него внимание.
    'builtin:wave': cursor * v.curiosity * 0.9 + v.affection * 0.2,
    // От скуки тянет двигаться.
    'builtin:walk': v.boredom * 0.6 * (input.canWalk ? 1 : 0),
    'builtin:fidget': v.boredom * 0.45 + 0.15,
    'builtin:stretch': v.boredom * 0.3 + (1 - v.energy) * 0.35,
    // Когда человека нет, персонаж устраивается поудобнее.
    'builtin:sit': (away ? 0.5 : 0.2) + (1 - v.energy) * 0.3,
    'builtin:lie': (away ? 0.45 : 0.05) + (1 - v.energy) * 0.4,
    // Покой — всегда допустимый запасной вариант.
    'builtin:stand': 0.25,
    'builtin:dance': input.music ? 1.5 : 0,
  }

  return Object.entries(scores)
    .filter(([action]) => input.available(action))
    // Повтор того же занятия подряд выглядит зависанием.
    .map(([action, score]) => ({ action, score: action === input.current ? score - 0.5 : score }))
    .filter((choice) => choice.score > 0)
    .sort((a, b) => b.score - a.score)
}

/**
 * Выбирает занятие: лучшее из близких по уместности.
 *
 * Берётся не строгий максимум, а случайный из тех, что почти так же хороши.
 * Строгий максимум при медленно меняющихся величинах давал бы одно и то же
 * занятие десятки раз подряд.
 */
export function choose(input: ChoiceInput, random: () => number): string {
  const ranked = rank(input)
  const best = ranked[0]
  if (!best) return 'builtin:stand'
  const close = ranked.filter((choice) => choice.score >= best.score - 0.2)
  const picked = close[Math.min(close.length - 1, Math.floor(random() * close.length))]
  return picked?.action ?? 'builtin:stand'
}
