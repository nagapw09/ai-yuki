/**
 * Сцена аватара: three.js + VRM (ТЗ §12).
 *
 * # Что здесь честно, а что приближение
 *
 * Blink, idle-анимации, выражения лица и переходы между состояниями — настоящие:
 * они считаются каждый кадр и зависят от времени, а не от заранее записанного
 * ролика.
 *
 * Windows и HTTP TTS передают громкость фактически проигрываемого аудио:
 * она управляет раскрытием рта. Это синхронизация по амплитуде, без фонем.
 * Для системного macOS TTS пока используется ритм слогов во время озвучки.
 */

import {
  VRMExpressionPresetName,
  VRMSpringBoneCollider,
  VRMSpringBoneColliderShapePlane,
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
import { isPoseClip, pickMotion, type Action, type MotionMap } from './motions'
import { acquireModel } from './model-cache'
import { avatarAnimationBytes } from '../bridge'

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
const framingCache = new WeakMap<VRM, Record<AvatarPose,Framing|null>>()

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

    const clip = createVRMAnimationClip(first, vrm)
    if (source.name === 'greeting') {
      const hips = vrm.humanoid.getNormalizedBoneNode('hips')
      const track = clip.tracks.find(track=>track.name === `${hips?.name}.position`)
      if (track) {
        const heights = Array.from(track.values).filter((_,index)=>index%3===1)
        const upright = Math.max(...heights)*0.9
        const start = heights.findIndex(value=>value>=upright)
        const from = track.times[Math.max(0,start)] ?? 0
        // This motion pack starts crouched. Use its standing wave, then return
        // to idle once; reversing the clip would make the character sit again.
        if (from > 0.2) for (const part of clip.tracks) part.trim(from,clip.duration).shift(-from)
        clip.resetDuration()
        for(let n=0;n<track.values.length;n+=3) {track.values[n]=hips?.position.x??0;track.values[n+2]=hips?.position.z??0}
      }
    }
    return mixer.clipAction(clip)
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
type Pose = { node: THREE.Object3D; quaternion: THREE.Quaternion; position: THREE.Vector3 }[]

function capturePose(vrm: VRM): Pose {
  const snapshot: Pose = []
  const root = vrm.humanoid?.normalizedHumanBonesRoot
  if (!root) return snapshot

  root.traverse((node) => {
    snapshot.push({ node, quaternion: node.quaternion.clone(), position: node.position.clone() })
  })

  return snapshot
}

function restorePose(snapshot: Pose): void {
  for (const { node, quaternion, position } of snapshot) {
    node.quaternion.copy(quaternion)
    node.position.copy(position)
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
/**
 * Насколько далеко уводится взгляд за курсором и как быстро он его догоняет.
 *
 * Дальность в единицах сцены на расстоянии метра от глаз: больше — и персонаж
 * начинает косить. Скорость подобрана так, чтобы взгляд ощущался вниманием, а
 * не прицелом: полный поворот занимает около трети секунды.
 */
const GAZE_REACH = 0.55
const GAZE_EASE = 3.2

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
  configureMotions: (map:MotionMap) => void
  hitTest: (x:number,y:number) => boolean
  /** Куда попадает точка окна: по голове, по телу или мимо. */
  touchZone: (x:number,y:number) => 'head' | 'body' | null
  /** Есть ли у модели такое выражение: наборы у авторов разные. */
  hasExpression: (name: string) => boolean
  setPointer: (x: number, y: number) => void
  activity: () => string
  setSpeech: (speaking: boolean, level: number | null) => void
  setDirection: (direction: number) => void
  contact: () => number
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
  model: string,
  pose: AvatarPose = 'portrait',
  animations: readonly string[] = [],
): Promise<AvatarScene> {
  const loadStarted = performance.now()
  const resource = await acquireModel(model)
  const vrm = resource.vrm
  const renderer = new THREE.WebGLRenderer({
    canvas,
    alpha: true,
    antialias: true,
  })
  renderer.setPixelRatio(resource.economy ? Math.min(window.devicePixelRatio, 1.25) : Math.min(window.devicePixelRatio, 2))
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

  scene.add(vrm.scene)

  const head = vrm.humanoid?.getNormalizedBoneNode('head')

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
  let measured = framingCache.get(vrm)
  if (!measured) {
    measured={portrait:measureUpperBody(vrm,head),full:measureWholeBody(vrm)}
    framingCache.set(vrm,measured)
  }
  const framings: Record<AvatarPose, Framing | null> = {
    portrait: measured.portrait ? {...measured.portrait} : null,
    full: measured.full ? {...measured.full} : null,
  }

  // Long hair, a tail, or a wide animation must not change a character's height.
  // The full-body camera keeps a common vertical fill; the wider overlay provides
  // room for hands. Portrait framing can still fit by width.
  const full = framings.full
  const standingFloor = full ? full.centerY - full.halfHeight : 0
  if (full) {
    const headroom = full.halfHeight * 0.2
    const bottomMargin = full.halfHeight * 0.05
    full.centerY += (headroom - bottomMargin) / 2
    full.halfHeight += (headroom + bottomMargin) / 2
    full.halfWidth = full.halfHeight * 0.55
  }
  const restingHip = vrm.humanoid.getNormalizedBoneNode('hips')?.getWorldPosition(new THREE.Vector3()).y ?? 0.8
  // A desktop surface is also a physical floor for hair and skirt springs.
  const floor = new VRMSpringBoneCollider(new VRMSpringBoneColliderShapePlane({normal:new THREE.Vector3(0,1,0)}))
  floor.position.y = standingFloor + 0.025
  scene.add(floor);floor.updateWorldMatrix(true,false)
  const floorGroup = {name:'yuki-surface',colliders:[floor]}
  for (const joint of vrm.springBoneManager?.joints ?? []) joint.colliderGroups.push(floorGroup)
  let surfaceY = standingFloor
  const pointer = {x:0,y:0}
  const raycaster = new THREE.Raycaster()

  /**
   * Куда попадает точка окна: `head`, `body` или `null`.
   *
   * Голова берётся из настоящей кости и проецируется на экран — так зона
   * совпадает с головой на любой модели и в любом кадрировании. Тело остаётся
   * прямоугольником: перебирать треугольники скина на каждый опрос курсора
   * незачем, а промах по руке ничего не ломает.
   */
  function touchZone(x:number, y:number): 'head' | 'body' | null {
    if (x < 0 || x > 1 || y < 0 || y > 1) return null
    const head = vrm.humanoid.getNormalizedBoneNode('head')
    if (head) {
      const point = head.getWorldPosition(new THREE.Vector3()).project(camera)
      const hx = (point.x + 1) / 2
      const hy = (1 - point.y) / 2
      // Радиус в долях окна: голова занимает примерно седьмую часть высоты.
      if (Math.hypot((x - hx) * 0.75, y - hy - 0.02) < 0.1) return 'head'
    }
    if (y < 0.1 && x > 0.15 && x < 0.85) return 'head'
    return x > 0.2 && x < 0.8 && y > 0.08 && y < 0.98 ? 'body' : null
  }

  const applyFraming = () => {
    const framing = framings[currentPose] ?? framings.portrait
    if (framing) placeCamera(camera, framing)
  }
  applyFraming()

  /**
   * Куда смотрит персонаж.
   *
   * Точка живёт перед камерой и сдвигается за курсором: когда мышь рядом,
   * взгляд идёт за ней, когда далеко или её нет — возвращается к человеку,
   * то есть в камеру. Именно слежение взглядом отличает предмет на экране от
   * существа в пространстве: без него аватар смотрит сквозь вас всегда.
   */
  const gaze = new THREE.Object3D()
  gaze.position.set(0, 0, -1)
  /** Текущее смещение взгляда, догоняющее курсор. */
  const gazeAt = {x: 0, y: 0}
  if (vrm.lookAt) {
    camera.add(gaze)
    scene.add(camera)
    vrm.lookAt.target = gaze
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
  let gesture: Action = 'stand'
  let motionMap:MotionMap = {}
  /** Как проигрывать: разрешённый режим и то, что попросили. */
  let playback:'once'|'loop'|'hold' = 'once'
  let wanted:'once'|'loop'|'hold'|'auto' = 'once'
  let frameTimer:ReturnType<typeof setTimeout>|undefined
  let renderedFrames=0
  let emotion = ''
  let emotionUntil = 0
  let speech = false
  let speechLevel: number | null = null
  let facing = 0
  const baseYaw = vrm.scene.rotation.y
  const hips = vrm.humanoid.getNormalizedBoneNode('hips')
  const feet = ['leftFoot','rightFoot'].map(name => vrm.humanoid.getNormalizedBoneNode(name as 'leftFoot' | 'rightFoot')).filter((v): v is THREE.Object3D => !!v)
  const feetRest = feet.map(foot => foot.getWorldPosition(new THREE.Vector3()).y)

  // ── Слои живости ────────────────────────────────────────────────────────
  //
  // Поверх любого клипа: дыхание, голова за взглядом, мелкие движения головы в
  // покое, кивки в разговоре, наклон головы в раздумье. Без них модель между
  // клипами застывала манекеном — клип двигает тело, а живым его делает то,
  // что никогда не останавливается (разбор Desktop Mate, §21, §35).
  //
  // Добавка ложится умножением поверх того, что записал микшер, и снимается в
  // начале следующего кадра. Иначе там, где клипа нет, повороты копились бы
  // кадр за кадром и голова уехала бы за спину.
  const layerBones = {
    head: vrm.humanoid.getNormalizedBoneNode('head'),
    neck: vrm.humanoid.getNormalizedBoneNode('neck'),
    chest: vrm.humanoid.getNormalizedBoneNode('upperChest') ?? vrm.humanoid.getNormalizedBoneNode('chest'),
    spine: vrm.humanoid.getNormalizedBoneNode('spine'),
  }
  const layerApplied = new Map<THREE.Object3D, THREE.Quaternion>()
  const layerEuler = new THREE.Euler()
  /** Сглаженные углы головы: цели меняются рывками, голова — нет. */
  const headLayer = {yaw: 0, pitch: 0, roll: 0}

  function undoLayers() {
    for (const [node, added] of layerApplied) node.quaternion.multiply(added.invert())
    layerApplied.clear()
  }

  function addLayer(node: THREE.Object3D | null, x: number, y: number, z: number) {
    if (!node) return
    const q = new THREE.Quaternion().setFromEuler(layerEuler.set(x, y, z))
    node.quaternion.multiply(q)
    const before = layerApplied.get(node)
    layerApplied.set(node, before ? before.multiply(q) : q)
  }

  const mixer = new THREE.AnimationMixer(vrm.scene)
  const actions = new Map<string, THREE.AnimationAction>()

  const loadingClips = new Set<string>()

  /** Играющая анимация; `null` — только своё движение. */
  let playing: THREE.AnimationAction | null = null
  let transitionFrom: Pose | null = null
  let transitionAt = 0

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

    // Stop stale actions: an old mixer track must not overwrite procedural arms.
    if (playing) {
      if (!next) {transitionFrom=capturePose(vrm);transitionAt=clock.elapsedTime}
      playing.stop()
    }

    if (next) {
      // «Авто» решается здесь: только тут известен сам клип. Поза — это один-два
      // ключевых кадра, и проигранная «один раз» она мелькает за сотые доли
      // секунды. Её надо удерживать, а цикл бега из двух десятков кадров —
      // проигрывать.
      playback = wanted === 'auto' ? (isPoseClip(next.getClip()) ? 'hold' : 'once') : wanted

      next.reset()
      // Незамкнутый клип идёт туда и обратно: так его конец всегда совпадает
      // с началом следующего повтора, и рывка на стыке нет.
      next.setLoop(playback==='loop' ? THREE.LoopRepeat : THREE.LoopOnce, playback==='loop' ? Infinity : 1)
      next.clampWhenFinished = true
      next.fadeIn(CROSSFADE).play()
    }

    playing = next

    // Возврат к своему движению: снимок восстанавливается сразу, а микшер
    // догасит собственный вклад за время перехода.
    if (!next) restorePose(restPose)
  }

  /** Что должно играть прямо сейчас: просьба важнее состояния. */
  function resolve() {
    apply(requested)
  }

  function play(name: string) {
    if (name.startsWith('emotion:')) {
      emotion = name.slice(8); emotionUntil = clock.elapsedTime + 4; return
    }
    if (name.startsWith('builtin:')) {
      const action=name.slice(8) as Action
      // Покой не перебирается на каждом тике: смена клипа покоя посреди
      // стояния выглядит как рывок, а не как разнообразие.
      const clip=action==='stand'&&gesture==='stand'&&requested?requested:pickMotion(action,animations,motionMap,requested)
      if (!clip) {if(action==='stand') {gesture='stand';requested='';apply('')} return}
      gesture=action
      wanted=action==='wave'||action==='stretch'||action==='fidget'?'once':action==='sit'||action==='lie'||action==='sleep'||action==='pose'?'hold':'loop'
      name=clip
    } else {
      gesture='stand'
      const mode=/^(once|loop|hold|auto):/.exec(name)
      // Без явного режима решаем по самому клипу: список движений отдаёт и позы,
      // и танцы, и одинаковое «один раз» подходит только вторым.
      wanted=(mode?.[1] as typeof wanted)||'auto'
      if(mode)name=name.slice(mode[0].length)
    }
    requested = name.trim()
    if (animations.includes(requested) && !actions.has(requested) && !loadingClips.has(requested)) {
      const loading = requested
      loadingClips.add(loading)
      void avatarAnimationBytes(loading).then(bytes => loadAnimation(vrm, mixer, {name:loading,bytes})).then(action => {
        if (state.running && action) { actions.set(loading, action); resolve() }
      }).catch(console.warn).finally(() => loadingClips.delete(loading))
    }
    resolve()
  }

  // Клип состояния начинается сразу: без него аватар стоял бы в покое до
  // первой просьбы, хотя движение у него уже есть.
  resolve()

  const clock = new THREE.Clock()
  mixer.addEventListener('finished', event => {
    if (event.action===playing && playback==='once') {gesture='stand';requested='';apply('');play('builtin:stand')}
  })

  function applyExpression(name: string, value: number) {
    vrm.expressionManager?.setValue(name, value)
  }

  function frame() {
    if (!state.running) return
    if(document.hidden || document.documentElement.dataset.paused) {clock.getDelta();frameTimer=setTimeout(frame,500);return}

    const delta = Math.min(clock.getDelta(), 0.1)
    const time = clock.elapsedTime
    const look = LOOKS[speech ? 'speaking' : gesture === 'sleep' ? 'sleeping' : state.current]

    // ── Взгляд ─────────────────────────────────────────────────────────────
    //
    // Спящий никуда не смотрит. В остальное время глаза идут за курсором с
    // задержкой: мгновенное слежение выглядит как прицел, а не как внимание.
    const wantX = look.asleep ? 0 : pointer.x * GAZE_REACH
    const wantY = look.asleep ? 0 : pointer.y * GAZE_REACH * 0.6
    gazeAt.x += (wantX - gazeAt.x) * Math.min(1, delta * GAZE_EASE)
    gazeAt.y += (wantY - gazeAt.y) * Math.min(1, delta * GAZE_EASE)
    gaze.position.set(gazeAt.x, gazeAt.y, -1)

    // ── Дыхание и покачивание ──────────────────────────────────────────────
    //
    // Только в покое: пока играет анимация, эти строки спорили бы с ней за те
    // же кости и превращали движение в дрожь. Мимика и моргание остаются —
    // они живут на других каналах и танцу не мешают.

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
    const expressionNames = new Set<string>([...ALL_EXPRESSIONS, ...state.weights.keys(), ...(time < emotionUntil ? [emotion] : [])])
    for (const name of expressionNames) {
      const expression = time < emotionUntil ? emotion : gesture === 'wave' || gesture === 'dance' ? 'happy' : look.expression
      const target = name === expression ? (time < emotionUntil || gesture === 'wave' || gesture === 'dance' ? 0.8 : look.weight) : 0
      const previous = state.weights.get(name) ?? 0
      // Экспоненциальное сглаживание: переход занимает доли секунды и не
      // зависит от частоты кадров.
      const next = previous + (target - previous) * Math.min(1, delta * EXPRESSION_EASE)
      if (target === 0 && next < 0.001 && !(ALL_EXPRESSIONS as readonly string[]).includes(name)) {
        state.weights.delete(name)
        applyExpression(name, 0)
      } else {
        state.weights.set(name, next)
        applyExpression(name, next)
      }
    }

    // ── Рот ────────────────────────────────────────────────────────────────
    if (speech || look.speaking) {
      // Настоящая громкость, если она есть: рот открывается ровно на звуке.
      // Так работает синтез, который проигрывает Yuki сама (`HttpTts`).
      const fresh = speechLevel !== null || time - state.audioLevelAt < LEVEL_FRESH

      // Иначе — слоговый ритм плюс вторая, более медленная волна: иначе рот
      // стучит как метроном и выглядит хуже, чем неподвижный. Это приближение,
      // и оно остаётся для системного синтеза, который звука не отдаёт.
      const syllable = Math.abs(Math.sin((time * Math.PI) / SYLLABLE))
      const phrase = 0.55 + 0.45 * Math.sin(time * 1.7)

      const openness = fresh
        ? Math.min(1, (speechLevel ?? state.audioLevel) * MOUTH_GAIN)
        : syllable * phrase
      applyExpression(VRMExpressionPresetName.Aa, openness * 0.7)
      applyExpression(VRMExpressionPresetName.Ih, openness * 0.25)
    } else {
      applyExpression(VRMExpressionPresetName.Aa, 0)
      applyExpression(VRMExpressionPresetName.Ih, 0)
    }

    // Микшер до vrm.update: он пишет в нормализованные кости, а vrm.update
    // переносит их в настоящий скелет. Обратный порядок отставал на кадр.
    undoLayers()
    mixer.update(delta)
    if (!playing) {
      if (transitionFrom) restorePose(restPose)
      if (transitionFrom) {
        const progress = Math.min(1,(time-transitionAt)/0.65)
        const ease = progress*progress*(3-2*progress)
        for (const from of transitionFrom) {
          from.node.quaternion.slerpQuaternions(from.quaternion,from.node.quaternion.clone(),ease)
          from.node.position.lerpVectors(from.position,from.node.position.clone(),ease)
        }
        if (progress>=1) transitionFrom=null
      }
    }
    // Слои поверх клипа. Танец и ходьба двигают всё тело сами, и добавка к
    // ним читалась бы дрожью; во сне голова лежит спокойно.
    const whole = gesture === 'dance' || gesture === 'walk'
    if (!whole) {
      const breath = Math.sin((time * Math.PI * 2) / 4.2)
      addLayer(layerBones.chest ?? layerBones.spine, breath * 0.018, 0, 0)
      addLayer(layerBones.spine, breath * 0.008, 0, 0)

      const asleep = look.asleep || gesture === 'sleep' || gesture === 'lie'
      // Голова идёт за взглядом слабее глаз: смотрят глазами, поворачиваются
      // головой только на то, что интересно по-настоящему.
      const busyClip = !!playing && gesture !== 'stand'
      const reach = asleep ? 0 : busyClip ? 0.5 : 1
      const yaw = gazeAt.x * 0.5 * reach + Math.sin(time * 0.37) * 0.03 * reach
      let pitch = -gazeAt.y * 0.35 * reach + Math.sin(time * 0.29 + 1) * 0.015 * reach
      let roll = Math.sin(time * 0.53 + 2) * 0.02 * reach
      if (speech && !asleep) {
        const level = speechLevel ?? state.audioLevel
        pitch += Math.min(1, level * 2) * 0.07 * Math.abs(Math.sin(time * 4.1)) + Math.sin(time * 2.3) * 0.015
        roll += Math.sin(time * 1.3) * 0.025
      }
      if (state.current === 'thinking' && !asleep) {
        roll += 0.12
        pitch -= 0.05
      }
      const ease = Math.min(1, delta * 4)
      headLayer.yaw += (yaw - headLayer.yaw) * ease
      headLayer.pitch += (pitch - headLayer.pitch) * ease
      headLayer.roll += (roll - headLayer.roll) * ease
      // Поворот делится между шеей и головой: одна голова на неподвижной шее
      // крутится как у совы.
      addLayer(layerBones.neck, headLayer.pitch * 0.4, headLayer.yaw * 0.4, headLayer.roll * 0.4)
      addLayer(layerBones.head, headLayer.pitch * 0.6, headLayer.yaw * 0.6, headLayer.roll * 0.6)
    }

    const desiredYaw = baseYaw + (gesture === 'walk' ? facing * Math.PI * 0.38 : 0)
    vrm.scene.rotation.y += (desiredYaw - vrm.scene.rotation.y) * Math.min(1, delta * 7)
    vrm.scene.rotation.z = 0
    vrm.scene.position.set(0, 0, 0)
    vrm.humanoid.update()
    vrm.scene.updateMatrixWorld(true)
    if (gesture === 'walk' && feet.length) {
      const lift = Math.min(...feet.map((foot,index) => foot.getWorldPosition(new THREE.Vector3()).y - (feetRest[index] ?? 0)))
      vrm.scene.position.y = -lift
    }
    // Contact follows the actual authored seated pelvis, not the standing height.
    const seat=gesture==='sit' && hips ? hips.getWorldPosition(new THREE.Vector3()).y - restingHip*0.055 : standingFloor
    surfaceY += (seat - surfaceY) * (1 - Math.exp(-delta * 12))
    vrm.update(delta)
    vrm.scene.updateMatrixWorld(true)
    renderer.render(scene, camera)
    renderedFrames++
    frameTimer=setTimeout(()=>requestAnimationFrame(frame), speech||gesture==='dance'||gesture==='walk'||gesture==='wave' ? 25 : 50)
  }

  requestAnimationFrame(frame)
  canvas.dataset.loadMs = String(Math.round(performance.now() - loadStarted))
  canvas.dataset.cached = String(resource.cached)
  if (import.meta.env.DEV) Object.assign(canvas, {yukiSeek: (seconds:number) => {if(playing){playing.time=seconds;mixer.update(0)}},yukiDebug: () => ({
    gesture, requested, playback, renderedFrames, model, loadingMs:canvas.dataset.loadMs, cached:resource.cached,
    bones:Object.fromEntries(['head','hips','leftHand','rightHand','leftFoot','rightFoot'].map(name => {
      const bone = vrm.humanoid.getNormalizedBoneNode(name as 'head')
      return [name,bone?.getWorldPosition(new THREE.Vector3()).toArray()]
    })), root:vrm.scene.position.toArray(), mouth:vrm.expressionManager?.getValue('aa'),
    expressionWeights: Object.fromEntries(vrm.expressionManager?.expressions.map(expression=>[expression.expressionName,expression.weight]) || []),
    economy:resource.economy,pixelRatio:renderer.getPixelRatio(),
    // Взгляд: есть ли он у модели вообще, куда смотрит и где курсор.
    lookAt:!!vrm.lookAt, pointer:{...pointer}, gazeAt:{...gazeAt},
    lookApplier:vrm.lookAt?.applier?.constructor?.name, lookAuto:vrm.lookAt?.autoUpdate,
    lookYaw:vrm.lookAt?.yaw, lookPitch:vrm.lookAt?.pitch,
    eyeBones:['leftEye','rightEye'].map(n=>!!vrm.humanoid.getNormalizedBoneNode(n as 'leftEye')),
    gazeWorld:gaze.getWorldPosition(new THREE.Vector3()).toArray().map(v=>+v.toFixed(3)),
  })})

  return {
    configureMotions: map=>{motionMap=map;if(gesture==='stand')play('builtin:stand')},
    hitTest: (x,y) => touchZone(x,y) !== null,
    /**
     * Есть ли у модели такое выражение.
     *
     * Наборы у авторов разные: у одной модели есть «удивление», у другой его
     * нет вовсе, и просьба показать несуществующее выражение молча ничего не
     * делает. Спрашивающий подбирает замену сам.
     */
    hasExpression: name => !!vrm.expressionManager?.getExpression(name),
    /**
     * Куда попал курсор: по голове, по телу или мимо.
     *
     * Голова ищется по настоящей кости, спроецированной на экран, а не по
     * доле окна: у разных моделей и кадрирований она в разных местах, и
     * «верхние десять процентов» на одной модели голова, а на другой — воздух.
     */
    touchZone,
    setPointer: (x,y) => {pointer.x=THREE.MathUtils.clamp(x,-1,1);pointer.y=THREE.MathUtils.clamp(y,-1,1)},
    activity: () => `builtin:${gesture}`,
    setSpeech: (speaking, level) => { speech = speaking; speechLevel = level },
    setDirection: direction => { facing = direction },
    contact: () => {
      // Anchor to a rest surface, never to a swinging foot.
      const point = new THREE.Vector3(0, surfaceY, 0)
      point.project(camera)
      return Math.max(0.05, Math.min(1, (1 - point.y) / 2))
    },
    setState: (next) => {
      state.current = next
      resolve()
    },
    setPose: (next) => {
      currentPose = next
      applyFraming()
    },
    play,
    clips: [...animations],
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
      clearTimeout(frameTimer)
      mixer.stopAllAction()
      mixer.uncacheRoot(vrm.scene)
      for (const joint of vrm.springBoneManager?.joints ?? []) joint.colliderGroups = joint.colliderGroups.filter(group=>group!==floorGroup)
      resource.release()
      renderer.dispose()
    },
  }
}
