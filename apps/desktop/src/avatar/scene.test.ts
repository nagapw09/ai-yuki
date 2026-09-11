/**
 * Поза и кадр аватара.
 *
 * Проверяется то, что сломалось живьём: модель стояла буквой Т и не влезала
 * в окно. Обе беды считаются обычной математикой без WebGL, поэтому их можно
 * сторожить тестом, а не глазами на каждой сборке.
 */

import type { VRM } from '@pixiv/three-vrm'
import * as THREE from 'three'
import { describe, expect, it } from 'vitest'

import { measureUpperBody, placeCamera, relaxArms } from './scene'

/** Рука из трёх костей, вытянутая вдоль X — ровно так лежит Т-поза в файле. */
function arm(side: number) {
  const upper = new THREE.Object3D()
  upper.position.set(0.18 * side, 1.4, 0)
  const lower = new THREE.Object3D()
  lower.position.set(0.26 * side, 0, 0)
  const hand = new THREE.Object3D()
  hand.position.set(0.24 * side, 0, 0)
  upper.add(lower)
  lower.add(hand)
  return { upper, lower, hand }
}

/**
 * Подделка VRM: `relaxArms` и `measureUpperBody` трогают только сцену и
 * гуманоид, поэтому всё остальное им незачем.
 *
 * `side` задаёт, в какую сторону смотрит модель: у VRM 0.x левая рука уходит
 * в минус X, у VRM 1.0 — в плюс. Обе стороны проверяются одним и тем же кодом.
 */
function model(side: number) {
  const scene = new THREE.Object3D()
  const left = arm(side)
  const right = arm(-side)

  const head = new THREE.Object3D()
  head.position.set(0, 1.4, 0)
  scene.add(head, left.upper, right.upper)

  const body = new THREE.Mesh(new THREE.BoxGeometry(0.4, 1.62, 0.3))
  body.position.set(0, 0.81, 0)
  scene.add(body)

  const bones: Record<string, THREE.Object3D> = {
    head,
    leftUpperArm: left.upper,
    leftLowerArm: left.lower,
    leftHand: left.hand,
    rightUpperArm: right.upper,
    rightLowerArm: right.lower,
    rightHand: right.hand,
  }

  const vrm = {
    scene,
    humanoid: { getNormalizedBoneNode: (name: string) => bones[name] ?? null },
    update: () => {},
  } as unknown as VRM

  return { vrm, head, left, right }
}

/** Мировая высота кисти после всех поворотов. */
function handHeight(hand: THREE.Object3D, scene: THREE.Object3D): number {
  scene.updateMatrixWorld(true)
  return hand.getWorldPosition(new THREE.Vector3()).y
}

describe('поза аватара', () => {
  // Знак поворота зависит от версии VRM, и жёсткое число одной модели руки
  // опускало, а другой поднимало. Обе стороны обязаны опуститься.
  for (const [name, side] of [['VRM 1.0', 1], ['VRM 0.x', -1]] as const) {
    it(`опускает руки из Т-позы (${name})`, () => {
      const { vrm, left, right } = model(side)
      const scene = vrm.scene

      const before = handHeight(left.hand, scene)
      relaxArms(vrm)

      expect(handHeight(left.hand, scene)).toBeLessThan(before - 0.3)
      expect(handHeight(right.hand, scene)).toBeLessThan(before - 0.3)
    })
  }

  it('не трогает руку, у которой нет костей ниже плеча', () => {
    const scene = new THREE.Object3D()
    const upper = new THREE.Object3D()
    upper.position.set(0.18, 1.4, 0)
    scene.add(upper)

    const vrm = {
      scene,
      humanoid: {
        getNormalizedBoneNode: (name: string) =>
          name === 'leftUpperArm' ? upper : null,
      },
      update: () => {},
    } as unknown as VRM

    relaxArms(vrm)

    // Повернуть её было бы гаданием: проверить результат не на чем.
    expect(upper.rotation.z).toBe(0)
  })
})

describe('кадр аватара', () => {
  it('вмещает модель и по высоте, и по ширине', () => {
    const { vrm, head } = model(1)
    relaxArms(vrm)

    const framing = measureUpperBody(vrm, head)
    expect(framing).not.toBeNull()

    // Узкое окно аватара: 320 на 480. Именно на нём срезало плечи.
    const camera = new THREE.PerspectiveCamera(28, 320 / 480, 0.1, 20)
    placeCamera(camera, framing!)

    const half = (camera.fov * Math.PI) / 360
    const visibleHalfHeight = Math.tan(half) * camera.position.z
    const visibleHalfWidth = visibleHalfHeight * camera.aspect

    expect(visibleHalfHeight).toBeGreaterThanOrEqual(framing!.halfHeight - 1e-6)
    expect(visibleHalfWidth).toBeGreaterThanOrEqual(framing!.halfWidth - 1e-6)
  })

  it('отодвигает камеру дальше, когда окно сужается', () => {
    const { vrm, head } = model(1)
    const framing = measureUpperBody(vrm, head)!

    const wide = new THREE.PerspectiveCamera(28, 1.6, 0.1, 20)
    const narrow = new THREE.PerspectiveCamera(28, 0.4, 0.1, 20)
    placeCamera(wide, framing)
    placeCamera(narrow, framing)

    expect(narrow.position.z).toBeGreaterThan(wide.position.z)
  })

  it('показывает голову, а не всю модель целиком', () => {
    const { vrm, head } = model(1)
    relaxArms(vrm)
    const framing = measureUpperBody(vrm, head)!

    const top = framing.centerY + framing.halfHeight / 1.06
    const bottom = framing.centerY - framing.halfHeight / 1.06

    expect(top).toBeGreaterThan(1.55) // макушка в кадре
    expect(bottom).toBeGreaterThan(0.4) // ноги — нет
  })

  it('молчит про модель без габаритов', () => {
    const vrm = { scene: new THREE.Object3D() } as unknown as VRM
    expect(measureUpperBody(vrm, null)).toBeNull()
  })
})
