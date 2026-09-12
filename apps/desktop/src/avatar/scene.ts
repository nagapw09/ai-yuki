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
import {
  createVRMAnimationClip,
  VRMAnimationLoaderPlugin,
  type VRMAnimation,
} from '@pixiv/three-vrm-animation'
import * as THREE from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'

import type { OrbState } from '../state/types'
import { ALL_EXPRESSIONS, LOOKS } from './states'

/** Насколько быстро выражение догоняет целевое (доля расхождения в секунду). */
const EXPRESSION_EASE = 6

/** Средняя длительность слога при озвучке, секунды. */
const SYLLABLE = 0.14

/**
 * Во сколько раз усилить громкость, прежде чем открывать рот.
 *
 * Среднеквадратичная громкость речи держится около 0.1: если открывать рот
 * ровно на неё, он будет едва шевелиться. Пятикратное усиление даёт на громких
 * слогах примерно половину раскрытия, что похоже на человека.
 */
const MOUTH_GAIN = 5

/**
 * Сколько секунд громкость считается свежей.
 *
 * По этому и определяется, настоящий ли lip-sync: если громкость приходит —
 * рот идёт за звуком, если перестала — за ритмом слогов. Отдельного
 * переключателя нет намеренно: он был бы третьим местом, где хранится то же
 * самое знание, и однажды разошёлся бы с действительностью.
 */
const LEVEL_FRESH = 0.25

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
  refreshBounds(vrm.scene)
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
 * Сбрасывает запомненные габариты мешей.
 *
 * Three.js считает габариты skinned-меша один раз и кладёт в `boundingBox`;
 * `Box3.setFromObject` потом берёт готовое значение и пересчитывает его только
 * если там `null`. Для нас это значит, что без сброса все замеры поз дают одну
 * и ту же позу — ту, что попалась первой. Именно поэтому кадр строился по позе
 * покоя, а поднятые руки уходили за края окна.
 *
 * Вызов не бесплатный: он заставляет пройтись по всем вершинам. Поэтому
 * используется только при замерах — при загрузке и при смене кадра, — а не
 * каждый кадр отрисовки.
 */
export function refreshBounds(root: THREE.Object3D): void {
  root.traverse((object) => {
    const mesh = object as THREE.Mesh & { boundingBox?: THREE.Box3 | null }
    if (mesh.boundingBox !== undefined) mesh.boundingBox = null
  })
}

/**
 * Считает кадр во весь рост.
 *
 * Это не «то же, только дальше»: аватар в полный рост — не собеседник в
 * портрете, а существо, стоящее на краю экрана, и кадр строится от пола до
 * макушки по габаритам модели. Запас снизу нулевой намеренно: ноги должны
 * стоять на нижней границе окна, иначе фигура висит в воздухе над панелью
 * задач вместо того, чтобы стоять на ней.
 */
export function measureWholeBody(vrm: VRM): Framing | null {
  refreshBounds(vrm.scene)
  const box = new THREE.Box3().setFromObject(vrm.scene)
  const height = box.max.y - box.min.y

  if (height <= 0.01) return null

  const top = box.max.y + height * 0.04
  const bottom = box.min.y

  return {
    centerY: (top + bottom) / 2,
    halfHeight: (top - bottom) / 2,
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

/**
 * Разбирает файл `.vrma` и превращает его в действие микшера.
 *
 * Возвращает `null`, а не бросает: один испорченный файл в папке не должен
 * оставлять аватар вообще без анимаций. Причина уходит в консоль окна —
 * человеку она ничего не скажет, а при разборе поможет.
 */
async function loadAnimation(
  vrm: VRM,
  mixer: THREE.AnimationMixer,
  source: AnimationSource,
): Promise<THREE.AnimationAction | null> {
  try {
    const loader = new GLTFLoader()
    loader.register((parser) => new VRMAnimationLoaderPlugin(parser))

    const gltf = await loader.parseAsync(source.bytes, '')
    const list = gltf.userData.vrmAnimations as VRMAnimation[] | undefined
    const first = list?.[0]
    if (!first) return null

    return mixer.clipAction(createVRMAnimationClip(first, vrm))
  } catch (error) {
    console.warn(`анимация «${source.name}» не разобралась`, error)
    return null
  }
}

/**
 * Замыкается ли клип сам на себя.
 *
 * Это решает, как его повторять. Клип «потянуться» начинается с рук внизу и
 * заканчивается руками вверху: повторённый по кругу, он на стыке роняет руки
 * рывком и снова поднимает — именно это и выглядит поломкой. Ходьба, наоборот,
 * замкнута, и разворачивать её назад было бы странно.
 *
 * Сравниваются первое и последнее значение каждой дорожки — то есть данные
 * самого клипа, без его проигрывания. Поза здесь не нужна, и трогать ради
 * проверки живую модель незачем.
 */
export function isCyclic(clip: THREE.AnimationClip, tolerance = 0.05): boolean {
  let worst = 0

  for (const track of clip.tracks) {
    const values = track.values
    const stride = track.getValueSize()
    if (values.length < stride * 2) continue

    for (let index = 0; index < stride; index += 1) {
      const first = values[index] ?? 0
      const last = values[values.length - stride + index] ?? 0
      worst = Math.max(worst, Math.abs(first - last))
    }
  }

  return worst <= tolerance
}

/** Снимок поворотов всех костей гуманоида. */
type Pose = { node: THREE.Object3D; quaternion: THREE.Quaternion }[]

function capturePose(vrm: VRM): Pose {
  const snapshot: Pose = []
  const root = vrm.humanoid?.normalizedHumanBonesRoot
  if (!root) return snapshot

  root.traverse((node) => {
    snapshot.push({ node, quaternion: node.quaternion.clone() })
  })

  return snapshot
}

function restorePose(snapshot: Pose): void {
  for (const { node, quaternion } of snapshot) {
    node.quaternion.copy(quaternion)
  }
}

/**
 * Расширяет кадр так, чтобы в него влезли габариты `reach`.
 *
 * Только расширяет: кадр, ставший от анимаций уже, означал бы, что в покое
 * модель обрезана ради движения, которого сейчас нет.
 */
export function widen(framing: Framing | null, reach: THREE.Box3): Framing | null {
  if (!framing) return null
  if (reach.isEmpty()) return framing

  const top = Math.max(framing.centerY + framing.halfHeight, reach.max.y)
  const bottom = Math.min(framing.centerY - framing.halfHeight, reach.min.y)

  return {
    centerY: (top + bottom) / 2,
    halfHeight: (top - bottom) / 2,
    halfWidth: Math.max(
      framing.halfWidth,
      Math.abs(reach.min.x),
      Math.abs(reach.max.x),
    ),
  }
}

/** Что показывать в окне. */
export type AvatarPose = 'portrait' | 'full'

/** Файл анимации: имя, под которым его просят, и содержимое. */
export interface AnimationSource {
  name: string
  bytes: ArrayBuffer
}

/** Сколько длится переход между покоем и анимацией, секунды. */
const CROSSFADE = 0.35

/**
 * Как клип находит своё состояние.
 *
 * Клип, названный именем состояния — `idle`, `thinking`, `sleeping`, — играет
 * сам, без всякой настройки. Иначе пришлось бы заводить таблицу «какой файл на
 * какое состояние», и человеку, положившему в папку восемь файлов, надо было бы
 * ещё восемь раз щёлкнуть. Имя файла и есть эта таблица.
 *
 * Своего движения Yuki при этом не теряет: состояние без клипа по-прежнему
 * живёт дыханием и покачиванием, которые считаются каждый кадр.
 */
function clipForState(state: OrbState): string {
  return state
}

export interface AvatarScene {
  /** Меняет состояние: мимика и темп подхватываются плавно. */
  setState: (state: OrbState) => void
  /** Переключает кадр: голова и торс или во весь рост. */
  setPose: (pose: AvatarPose) => void
  /**
   * Проигрывает анимацию по имени; пустое имя возвращает в покой.
   *
   * Клип повторяется, пока его не остановят: у танца нет естественного конца,
   * а «сыграть один раз и замереть» выглядит обрывом.
   */
  play: (name: string) => void
  /** Имена загруженных анимаций — в том порядке, в каком их дали. */
  readonly clips: readonly string[]
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
  pose: AvatarPose = 'portrait',
  animations: readonly AnimationSource[] = [],
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

  let currentPose: AvatarPose = pose

  // Кадры считаются ниже, после загрузки анимаций: их размах входит в замер.
  let framings: Record<AvatarPose, Framing | null> = {
    portrait: measureUpperBody(vrm, head),
    full: measureWholeBody(vrm),
  }

  const applyFraming = () => {
    const framing = framings[currentPose] ?? framings.portrait
    if (framing) placeCamera(camera, framing)
  }
  applyFraming()

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
    /** Когда громкость обновляли последний раз, по часам сцены. */
    audioLevelAt: -1,
    /** Текущие веса выражений — они догоняют целевые, а не переключаются рывком. */
    weights: new Map<string, number>(),
    /** Момент следующего моргания. */
    nextBlink: 1.5,
    blinkPhase: -1,
    running: true,
  }

  // ── Анимации ───────────────────────────────────────────────────────────────
  //
  // Поза покоя запоминается до первой анимации: клип перезаписывает повороты
  // костей, и после остановки без этого снимка руки остались бы там, где их
  // бросил танец, — то есть в произвольном месте, а не вдоль тела.
  const restPose = capturePose(vrm)

  const mixer = new THREE.AnimationMixer(vrm.scene)
  const actions = new Map<string, THREE.AnimationAction>()

  for (const source of animations) {
    const action = await loadAnimation(vrm, mixer, source)
    if (action) actions.set(source.name, action)
  }

  // Кадр пересчитывается по размаху анимаций.
  //
  // Замер по одной позе покоя обрезал поднятые руки: клип «потянуться»
  // выводит кисти выше макушки и шире плеч, а камера стояла так, будто руки
  // всегда внизу. Теперь каждый клип прогоняется по нескольким моментам, и
  // кадр строится по самому размашистому из них — тогда за края не выходит
  // ничто и никогда.
  //
  // Цена — аватар немного мельче в покое. Это дешевле, чем исчезающие кисти:
  // обрезанная рука читается как поломка, а полсантиметра запаса не читается
  // вовсе.
  if (actions.size > 0) {
    const reach = new THREE.Box3()

    for (const action of actions.values()) {
      const clip = action.getClip()
      action.reset().play()

      // Десять моментов на клип: чаще — лишняя работа при загрузке, реже —
      // можно проскочить мимо самой размашистой позы.
      for (let step = 0; step <= 10; step += 1) {
        mixer.setTime((clip.duration * step) / 10)
        vrm.update(0)
        vrm.scene.updateMatrixWorld(true)
        refreshBounds(vrm.scene)
        reach.union(new THREE.Box3().setFromObject(vrm.scene))
      }

      action.stop()
    }

    // Возврат в покой: после прогона кости остались там, где их бросил
    // последний клип.
    mixer.setTime(0)
    restorePose(restPose)
    vrm.update(0)
    vrm.scene.updateMatrixWorld(true)

    framings = {
      portrait: measureUpperBody(vrm, head),
      full: widen(measureWholeBody(vrm), reach),
    }

    applyFraming()
  }

  /** Играющая анимация; `null` — только своё движение. */
  let playing: THREE.AnimationAction | null = null

  /**
   * Анимация, которую попросили явно.
   *
   * Она важнее состояния: если человек сказал «потанцуй», а Yuki в этот момент
   * что-то обдумывает, танец не должен подменяться клипом раздумий. Пустая
   * строка возвращает управление состоянию.
   */
  let requested = ''

  function apply(name: string) {
    const next = name ? actions.get(name) ?? null : null

    if (next === playing) return

    if (playing) playing.fadeOut(CROSSFADE)

    if (next) {
      next.reset()
      // Незамкнутый клип идёт туда и обратно: так его конец всегда совпадает
      // с началом следующего повтора, и рывка на стыке нет.
      next.setLoop(
        isCyclic(next.getClip()) ? THREE.LoopRepeat : THREE.LoopPingPong,
        Infinity,
      )
      next.fadeIn(CROSSFADE).play()
    }

    playing = next

    // Возврат к своему движению: снимок восстанавливается сразу, а микшер
    // догасит собственный вклад за время перехода.
    if (!next) restorePose(restPose)
  }

  /** Что должно играть прямо сейчас: просьба важнее состояния. */
  function resolve() {
    apply(requested || clipForState(state.current))
  }

  function play(name: string) {
    requested = name.trim()
    resolve()
  }

  // Клип состояния начинается сразу: без него аватар стоял бы в покое до
  // первой просьбы, хотя движение у него уже есть.
  resolve()

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
    //
    // Только в покое: пока играет анимация, эти строки спорили бы с ней за те
    // же кости и превращали движение в дрожь. Мимика и моргание остаются —
    // они живут на других каналах и танцу не мешают.
    if (!playing) {
      const breath = Math.sin(time * Math.PI * 2 * look.breath)
      if (chest) chest.rotation.x = breath * 0.02
      if (spine) spine.rotation.z = Math.sin(time * 0.7) * 0.01 * look.sway
      if (head) {
        head.rotation.y = Math.sin(time * 0.53) * 0.04 * look.sway
        head.rotation.x = Math.sin(time * 0.41) * 0.03 * look.sway
        // Спящий аватар роняет голову — это читается даже без выражения лица.
        if (look.asleep) head.rotation.x += 0.18
      }
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
      // Настоящая громкость, если она есть: рот открывается ровно на звуке.
      // Так работает синтез, который проигрывает Yuki сама (`HttpTts`).
      const fresh = time - state.audioLevelAt < LEVEL_FRESH

      // Иначе — слоговый ритм плюс вторая, более медленная волна: иначе рот
      // стучит как метроном и выглядит хуже, чем неподвижный. Это приближение,
      // и оно остаётся для системного синтеза, который звука не отдаёт.
      const syllable = Math.abs(Math.sin((time * Math.PI) / SYLLABLE))
      const phrase = 0.55 + 0.45 * Math.sin(time * 1.7)

      const openness = fresh
        ? Math.min(1, state.audioLevel * MOUTH_GAIN)
        : syllable * phrase
      applyExpression(VRMExpressionPresetName.Aa, openness * 0.7)
      applyExpression(VRMExpressionPresetName.Ih, openness * 0.25)
    } else {
      applyExpression(VRMExpressionPresetName.Aa, 0)
      applyExpression(VRMExpressionPresetName.Ih, 0)
    }

    // Микшер до vrm.update: он пишет в нормализованные кости, а vrm.update
    // переносит их в настоящий скелет. Обратный порядок отставал на кадр.
    mixer.update(delta)
    vrm.update(delta)
    renderer.render(scene, camera)
    requestAnimationFrame(frame)
  }

  requestAnimationFrame(frame)

  return {
    setState: (next) => {
      state.current = next
      resolve()
    },
    setPose: (next) => {
      currentPose = next
      applyFraming()
    },
    play,
    clips: [...actions.keys()],
    setAudioLevel: (level) => {
      state.audioLevel = Math.max(0, Math.min(1, level))
      state.audioLevelAt = clock.elapsedTime
    },
    resize: (width, height) => {
      renderer.setSize(width, height, false)
      camera.aspect = width / Math.max(height, 1)
      camera.updateProjectionMatrix()
      // Окно можно тянуть за угол, и узкое требует другого расстояния, чем
      // широкое. Без пересчёта аватар вылезал бы за края после каждого
      // изменения размера.
      applyFraming()
    },
    dispose: () => {
      state.running = false
      mixer.stopAllAction()
      VRMUtils.deepDispose(vrm.scene)
      renderer.dispose()
    },
  }
}
