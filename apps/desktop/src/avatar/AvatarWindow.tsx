import { getCurrentWindow } from '@tauri-apps/api/window'
import { useEffect, useRef, useState } from 'react'

import {
  avatarModelBytes,
  avatarRememberPlacement,
  avatarStatus,
  AVATAR_EVENT,
  AVATAR_POSE_EVENT,
  type AvatarPose,
  type AvatarSignal,
} from '../bridge'
import { listen } from '@tauri-apps/api/event'

import type { AvatarScene } from './scene'
import './AvatarWindow.css'

/**
 * Окно аватара (ТЗ §12).
 *
 * Оно ничего не решает: состояние приходит событием из главного окна, модель —
 * командой из Rust. Всё, что здесь есть, — рисование и перетаскивание.
 *
 * Без выбранной модели окно не остаётся пустым: показывается тот же автомат
 * состояний в виде свечения. Пустое прозрачное окно поверх всех выглядело бы
 * как сбой, а не как «модель не выбрана».
 */
export function AvatarWindow() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null)
  const sceneRef = useRef<AvatarScene | null>(null)
  const [state, setState] = useState<AvatarSignal['state']>('idle')
  const [problem, setProblem] = useState<string | null>(null)

  // Состояние и громкость приходят из главного окна одним событием.
  useEffect(() => {
    const pending = listen<AvatarSignal>(AVATAR_EVENT, (event) => {
      setState(event.payload.state)
      sceneRef.current?.setState(event.payload.state)
      sceneRef.current?.setAudioLevel(event.payload.audioLevel)
    })
    return () => {
      void pending.then((unlisten) => unlisten())
    }
  }, [])

  // Кадр меняется из настроек в главном окне и приходит событием.
  useEffect(() => {
    const pending = listen<AvatarPose>(AVATAR_POSE_EVENT, (event) => {
      sceneRef.current?.setPose(event.payload)
    })
    return () => {
      void pending.then((unlisten) => unlisten())
    }
  }, [])

  // Модель грузится один раз при открытии окна.
  useEffect(() => {
    let disposed = false

    void (async () => {
      const canvas = canvasRef.current
      if (!canvas) return

      try {
        const bytes = await avatarModelBytes()
        if (disposed) return

        // Кадр спрашиваем до создания сцены: поставить камеру сразу дешевле,
        // чем показать портрет и через кадр переставить на полный рост.
        const pose = await avatarStatus()
          .then((status) => status.pose)
          .catch(() => 'portrait' as AvatarPose)
        if (disposed) return

        // three.js весит больше всего остального интерфейса вместе взятого.
        // Главному окну он не нужен, а бюджет запуска из ТЗ §37 общий —
        // поэтому сцена грузится отдельным куском и только здесь.
        const { createScene } = await import('./scene')
        if (disposed) return

        const scene = await createScene(canvas, bytes, pose)
        if (disposed) {
          scene.dispose()
          return
        }

        sceneRef.current = scene
        scene.resize(canvas.clientWidth, canvas.clientHeight)
        setProblem(null)
      } catch (error) {
        // Причину показываем прямо в окне: аватар — не то место, куда
        // пользователь пойдёт искать консоль.
        setProblem(describe(error))
      }
    })()

    return () => {
      disposed = true
      sceneRef.current?.dispose()
      sceneRef.current = null
    }
  }, [])

  // Размер окна меняет пользователь, и рендер обязан за ним успевать.
  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return

    const observer = new ResizeObserver(() => {
      sceneRef.current?.resize(canvas.clientWidth, canvas.clientHeight)
    })
    observer.observe(canvas)
    return () => observer.disconnect()
  }, [])

  // Положение и размер запоминаются: аватар должен возвращаться туда, где его
  // оставили, а не в середину экрана при каждом запуске.
  useEffect(() => {
    const window = getCurrentWindow()
    const remember = () => void avatarRememberPlacement().catch(() => {})

    const moved = window.onMoved(remember)
    const resized = window.onResized(remember)

    return () => {
      void moved.then((unlisten) => unlisten())
      void resized.then((unlisten) => unlisten())
    }
  }, [])

  return (
    <div className="avatar" data-state={state}>
      {/* Перетаскивание за само окно: рамки у него нет, и хвататься больше не за что. */}
      <div className="avatar__drag" data-tauri-drag-region />

      <canvas ref={canvasRef} className="avatar__canvas" />

      {problem && (
        <div className="avatar__fallback">
          <div className="avatar__orb" />
          <p className="avatar__note">{problem}</p>
        </div>
      )}
    </div>
  )
}

function describe(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return 'не удалось показать аватар'
}
