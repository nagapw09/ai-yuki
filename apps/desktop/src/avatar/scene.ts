/**
 * Сцена аватара: three.js + VRM (ТЗ §12).
 *
 * # Что здесь честно, а что приближение
 *
 * Blink, idle-анимации, выражения лица и переходы между состояниями — настоящие:
 * они считаются каждый кадр и зависят от времени, а не от заранее записанного
 * ролика.
 *
 * Lip-sync — приближение, и это надо называть своим именем. Синтез речи у Yuki
 * системный (SAPI на Windows, AVSpeechSynthesizer на macOS), и звуковой буфер
 * оттуда не приходит: анализировать нечего. Поэтому рот двигается по фазе речи —
 * пока состояние SPEAKING, губы отрабатывают правдоподобный ритм слогов, а
 * громкость микрофона используется, когда она есть. Настоящий lip-sync по
 * амплитуде появится, когда синтез будет проигрываться самим приложением.
 */

import {
  VRMExpressionPresetName,
  VRMLoaderPlugin,
  VRMUtils,
  type VRM,
} from '@pixiv/three-vrm'
import * as THREE from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'

import type { OrbState } from '../state/types'
import { ALL_EXPRESSIONS, LOOKS } from './states'

/** Насколько быстро выражение догоняет целевое (доля расхождения в секунду). */
const EXPRESSION_EASE = 6

/** Средняя длительность слога при озвучке, секунды. */
const SYLLABLE = 0.14

/** Насколько руки опускаются из Т-позы, радианы (около 72 градусов). */
const ARM_DOWN = 1.26

/** Небольшой сгиб в локте: прямая рука выглядит палкой. */
const ELBOW_BEND = 0.14

/**
 * Опускает руки вдоль тела.
 *
 * В файле VRM модель хранится в Т-позе — так требует формат, иначе скелеты
 * разных моделей нельзя было бы менять местами. Но живой аватар, стоящий
 * буквой Т, выглядит манекеном, и руки, раскинутые на полтора метра, ещё и не
 * влезают в узкое окно.
 *
 * Сторона поворота определяется пробой, а не записана числом. У VRM 0.x модель
 * смотрит в минус Z, у VRM 1.0 — в плюс Z, и один и тот же угол одной модели
 * руки опускает, а другой поднимает. Пробный поворот на малый угол показывает,
 * куда поехала кисть; настоящий идёт в ту сторону, где она оказалась ниже.
 */
export function relaxArms(vrm: VRM): void {
  const humanoid = vrm.humanoid
  if (!humanoid) return

  const sides = [
    ['leftUpperArm', 'leftLowerArm', 'leftHand'],
    ['rightUpperArm', 'rightLowerArm', 'rightHand'],
  ] as const

  const probe = new THREE.Vector3()

  for (const [upperName, lowerName, handName] of sides) {
    const upper = humanoid.getNormalizedBoneNode(upperName)
    const lower = humanoid.getNormalizedBoneNode(lowerName)
    const tip = humanoid.getNormalizedBoneNode(handName) ?? lower

    // Без кости дальше по руке пробу ставить не на чем: сам плечевой сустав
    // от собственного поворота не сдвигается. Такую руку лучше оставить как
    // есть, чем повернуть наугад.
    if (!upper || !tip) continue

    vrm.scene.updateMatrixWorld(true)
    const before = tip.getWorldPosition(probe).y

    upper.rotation.z = 0.2
    vrm.scene.updateMatrixWorld(true)
    const after = tip.getWorldPosition(probe).y

    const down = after < before ? 1 : -1
    upper.rotation.z = down * ARM_DOWN
    if (lower) lower.rotation.z = down * ELBOW_BEND
  }

  vrm.update(0)
  vrm.scene.updateMatrixWorld(true)
}

/** Какой кусок мира должна показать камера. */
export interface Framing {
  centerY: number
  halfHeight: number
  halfWidth: number
}

/**
 * Считает, что показывать: голову и торс примерно до пояса.
 *
 * Замер идёт по границам самой модели, а не по числу из головы: рост VRM-моделей
 * различается вдвое, и фиксированная камера одну обрезала бы по подбородок, а
 * другую показала точкой. Кость головы используется, если она есть и её
 * положение осмысленно; иначе работает оценка по габаритам — так кадрируются и
 * модели с нестандартным скелетом.
 *
 * Ширина меряется отдельно и честно, по габаритам модели: окно аватара узкое,
 * и кадр, подогнанный только по высоте, срезает плечи и руки.
 */
export function measureUpperBody(vrm: VRM, head: THREE.Object3D | null | undefined): Framing | null {
  const box = new THREE.Box3().setFromObject(vrm.scene)
  const height = box.max.y - box.min.y

  // Модель без габаритов — ставить камеру некуда; лучше оставить её там, где
  // она есть, чем отправить внутрь меша.
  if (height <= 0.01) return null

  const headY = head ? head.getWorldPosition(new THREE.Vector3()).y : 0

  // Кость принимается, только если она похожа на голову: выше середины роста и
  // не выше макушки.
  const plausible = headY > box.min.y + height * 0.5 && headY <= box.max.y + 0.05

  // Кость головы стоит у основания черепа, а не на уровне глаз: от неё до
  // макушки ещё сантиметров двадцать причёски. Кадр строится от макушки вниз,
  // иначе волосы срезает верхним краем.
  const top = box.max.y
  const headSize = plausible ? top - headY : height * 0.13
  const bottom = plausible ? headY - headSize * 2.4 : top - height * 0.45

  // Запас в паре процентов: модель, упирающаяся в края кадра, выглядит
  // обрезанной даже когда попала целиком.
  return {
    centerY: (top + bottom) / 2,
    halfHeight: ((top - bottom) / 2) * 1.06,
    halfWidth: Math.max(Math.abs(box.min.x), Math.abs(box.max.x)) * 1.06,
  }
}

/**
 * Отодвигает камеру на расстояние, с которого кадр влезает целиком.
 *
 * Считаются оба требования — по высоте и по ширине — и берётся большее.
 * Учитывать только высоту нельзя: окно аватара вытянутое, по горизонтали в
 * него помещается заметно меньше, и именно там срезало руки.
 */
export function placeCamera(camera: THREE.PerspectiveCamera, framing: Framing): void {
  const half = (camera.fov * Math.PI) / 360
  const byHeight = framing.halfHeight / Math.tan(half)
  const byWidth = framing.halfWidth / (Math.tan(half) * Math.max(camera.aspect, 0.01))

  camera.position.set(0, framing.centerY, Math.max(byHeight, byWidth))
  camera.lookAt(0, framing.centerY, 0)
  camera.updateProjectionMatrix()
}

export interface AvatarScene {
  /** Меняет состояние: мимика и темп подхватываются плавно. */
  setState: (state: OrbState) => void
  /** Громкость 0…1, если она известна (микрофон в режиме LISTENING). */
  setAudioLevel: (level: number) => void
  /** Подгоняет рендер под новый размер окна. */
  resize: (width: number, height: number) => void
  dispose: () => void
}

/**
 * Создаёт сцену и запускает цикл отрисовки.
 *
 * Модель приходит байтами: файл читает Rust, а не WebView, — так не нужен
 * файловый протокол и не приходится ослаблять CSP ради одной модели.
 */
export async function createScene(
  canvas: HTMLCanvasElement,
  model: ArrayBuffer,
): Promise<AvatarScene> {
  const renderer = new THREE.WebGLRenderer({
    canvas,
    alpha: true,
    antialias: true,
  })
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
  // Прозрачный фон — обязательное условие окна без рамки: иначе вокруг аватара
  // будет висеть чёрный прямоугольник.
  renderer.setClearColor(0x000000, 0)
  // Цветовое пространство задаём явно: умолчание менялось между версиями three,
  // и модель, собранная под sRGB, в линейном выводе выглядит выцветшей.
  renderer.outputColorSpace = THREE.SRGBColorSpace

  const scene = new THREE.Scene()

  const camera = new THREE.PerspectiveCamera(
    28,
    canvas.clientWidth / Math.max(canvas.clientHeight, 1),
    0.1,
    20,
  )

  // Свет мягкий и с двух сторон: одиночный источник на маленьком окне даёт
  // резкую границу тени поперёк лица.
  scene.add(new THREE.AmbientLight(0xffffff, 1.4))
  const key = new THREE.DirectionalLight(0xffffff, 1.2)
  key.position.set(1, 2, 2)
  scene.add(key)
  const rim = new THREE.DirectionalLight(0x8ab4ff, 0.5)
  rim.position.set(-1.5, 1, -1)
  scene.add(rim)

  const loader = new GLTFLoader()
  loader.register((parser) => new VRMLoaderPlugin(parser))

  const gltf = await loader.parseAsync(model, '')
  const loaded = gltf.userData.vrm as VRM | undefined
  if (!loaded) {
    renderer.dispose()
    throw new Error('это не VRM-модель')
  }
  const vrm: VRM = loaded

  // Рекомендованная подготовка: убирает неиспользуемые вершины и объединяет
  // скелеты. На маленьком окне это разница между 60 и 30 кадрами.
  VRMUtils.removeUnnecessaryVertices(gltf.scene)
  VRMUtils.combineSkeletons(gltf.scene)
  VRMUtils.combineMorphs(vrm)

  // Разворот делает библиотека, а не мы: 180 градусов нужны только моделям
  // VRM 0.x, а у VRM 1.0 перёд уже направлен к зрителю. Безусловный поворот
  // показывал бы половину моделей спиной.
  VRMUtils.rotateVRM0(vrm)

  // Части тела не должны исчезать при движении костей. Three.js считает объём
  // отсечения по позе покоя, и поднятая рука или наклон головы выводят кусок
  // меша за эту границу — он пропадает целиком. Для аватара в маленьком окне
  // экономия на отсечении не стоит исчезающих рук.
  vrm.scene.traverse((object) => {
    object.frustumCulled = false
  })

  scene.add(vrm.scene)

  const head = vrm.humanoid?.getNormalizedBoneNode('head')
  const spine = vrm.humanoid?.getNormalizedBoneNode('spine')
  const chest = vrm.humanoid?.getNormalizedBoneNode('chest') ?? spine

  // Первое обновление до замера: нормализованный скелет three-vrm получает
  // настоящие положения только после него, а матрицы мира — только после
  // updateMatrixWorld. Без этих двух строк замер возвращал положение кости
  // в покое относительно родителя — около нуля вместо полутора метров, — и
  // камера оказывалась на уровне пола, показывая ноги вместо лица.
  vrm.update(0)
  vrm.scene.updateMatrixWorld(true)

  // Руки опускаем до замера: в Т-позе модель втрое шире, и кадр, посчитанный
  // по ней, отодвинул бы камеру далеко назад ради пустоты по бокам.
  relaxArms(vrm)

  const framing = measureUpperBody(vrm, head)
  if (framing) placeCamera(camera, framing)

  // Взгляд следует за камерой: аватар смотрит на пользователя, а не сквозь него.
  if (vrm.lookAt) {
    const target = new THREE.Object3D()
    camera.add(target)
    target.position.set(0, 0, -1)
    scene.add(camera)
    vrm.lookAt.target = target
  }

  const state = {
    current: 'idle' as OrbState,
    audioLevel: 0,
    /** Текущие веса выражений — они догоняют целевые, а не переключаются рывком. */
    weights: new Map<string, number>(),
    /** Момент следующего моргания. */
    nextBlink: 1.5,
    blinkPhase: -1,
    running: true,
  }

  const clock = new THREE.Clock()

  function applyExpression(name: string, value: number) {
    vrm.expressionManager?.setValue(name, value)
  }

  function frame() {
    if (!state.running) return

    const delta = Math.min(clock.getDelta(), 0.1)
    const time = clock.elapsedTime
    const look = LOOKS[state.current]

    // ── Дыхание и покачивание ──────────────────────────────────────────────
    const breath = Math.sin(time * Math.PI * 2 * look.breath)
    if (chest) chest.rotation.x = breath * 0.02
    if (spine) spine.rotation.z = Math.sin(time * 0.7) * 0.01 * look.sway
    if (head) {
      head.rotation.y = Math.sin(time * 0.53) * 0.04 * look.sway
      head.rotation.x = Math.sin(time * 0.41) * 0.03 * look.sway
      // Спящий аватар роняет голову — это читается даже без выражения лица.
      if (look.asleep) head.rotation.x += 0.18
    }

    // ── Моргание ───────────────────────────────────────────────────────────
    if (look.asleep) {
      applyExpression(VRMExpressionPresetName.Blink, 1)
    } else {
      if (state.blinkPhase >= 0) {
        state.blinkPhase += delta
        // Моргание длится около 120 мс: закрыть и открыть.
        const progress = state.blinkPhase / 0.12
        applyExpression(
          VRMExpressionPresetName.Blink,
          progress >= 1 ? 0 : Math.sin(progress * Math.PI),
        )
        if (progress >= 1) {
          state.blinkPhase = -1
          // Интервал случайный: ровный ритм выглядит механическим.
          state.nextBlink = time + 2 + Math.random() * 4
        }
      } else if (time >= state.nextBlink) {
        state.blinkPhase = 0
      }
    }

    // ── Выражение лица ─────────────────────────────────────────────────────
    for (const name of ALL_EXPRESSIONS) {
      const target = name === look.expression ? look.weight : 0
      const previous = state.weights.get(name) ?? 0
      // Экспоненциальное сглаживание: переход занимает доли секунды и не
      // зависит от частоты кадров.
      const next = previous + (target - previous) * Math.min(1, delta * EXPRESSION_EASE)
      state.weights.set(name, next)
      applyExpression(name, next)
    }

    // ── Рот ────────────────────────────────────────────────────────────────
    if (look.speaking) {
      // Слоговый ритм плюс вторая, более медленная волна — иначе рот стучит
      // как метроном и выглядит хуже, чем неподвижный.
      const syllable = Math.abs(Math.sin((time * Math.PI) / SYLLABLE))
      const phrase = 0.55 + 0.45 * Math.sin(time * 1.7)
      const openness = Math.max(state.audioLevel, syllable * phrase)
      applyExpression(VRMExpressionPresetName.Aa, openness * 0.7)
      applyExpression(VRMExpressionPresetName.Ih, openness * 0.25)
    } else {
      applyExpression(VRMExpressionPresetName.Aa, 0)
      applyExpression(VRMExpressionPresetName.Ih, 0)
    }

    vrm.update(delta)
    renderer.render(scene, camera)
    requestAnimationFrame(frame)
  }

  requestAnimationFrame(frame)

  return {
    setState: (next) => {
      state.current = next
    },
    setAudioLevel: (level) => {
      state.audioLevel = Math.max(0, Math.min(1, level))
    },
    resize: (width, height) => {
      renderer.setSize(width, height, false)
      camera.aspect = width / Math.max(height, 1)
      camera.updateProjectionMatrix()
      // Окно можно тянуть за угол, и узкое требует другого расстояния, чем
      // широкое. Без пересчёта аватар вылезал бы за края после каждого
      // изменения размера.
      if (framing) placeCamera(camera, framing)
    },
    dispose: () => {
      state.running = false
      VRMUtils.deepDispose(vrm.scene)
      renderer.dispose()
    },
  }
}
