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
  vrm.scene.rotation.y = Math.PI // VRM смотрит от зрителя; разворачиваем к нему
  scene.add(vrm.scene)

  const head = vrm.humanoid?.getNormalizedBoneNode('head')
  const spine = vrm.humanoid?.getNormalizedBoneNode('spine')
  const chest = vrm.humanoid?.getNormalizedBoneNode('chest') ?? spine

  // Кадрируем по голове: рост моделей различается, и фиксированная камера
  // одну обрезала бы по подбородок, а другую показала бы точкой.
  const headHeight = head ? head.getWorldPosition(new THREE.Vector3()).y : 1.4
  camera.position.set(0, headHeight - 0.02, 1.65)
  camera.lookAt(0, headHeight - 0.08, 0)

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
    },
    dispose: () => {
      state.running = false
      VRMUtils.deepDispose(vrm.scene)
      renderer.dispose()
    },
  }
}
