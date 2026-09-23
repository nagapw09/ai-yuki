import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen } from '@tauri-apps/api/event'
import { useCallback, useEffect, useRef, useState } from 'react'
import {
  avatarAnimations, avatarRememberPlacement,
  avatarStatus, avatarClose, avatarPlay, avatarSetAnchor, settingGet, companionMotionTick, companionContext, companionPointer, voicePlayback, settingSet,
  AVATAR_EVENT, AVATAR_PLAY_EVENT, AVATAR_POSE_EVENT, type AvatarSignal, type AvatarPose, companionAttach, companionSurfaces,
} from '../bridge'
import type { AvatarScene } from './scene'
import { CompanionBehavior, type BehaviorMode, type CompanionContext } from './behavior'
import { assignedMotion, parseMotionMap, type Action, type MotionMap } from './motions'
import { parseVitals } from './character'
import './AvatarWindow.css'

/**
 * Сколько пикселей нажатие может «плыть», оставаясь касанием.
 *
 * Мышь дрожит под пальцем всегда, а на ноутбучном тачпаде — заметно. Ноль
 * означал бы, что персонажа нельзя потрогать, не сдвинув окно.
 */
const DRAG_SLOP = 5

export function AvatarWindow() {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const sceneRef = useRef<AvatarScene | null>(null)
  const latest = useRef<AvatarSignal>({ state: 'idle', audioLevel: 0 })
  const motion = useRef('stand')
  const target = useRef<number | null>(null)
  const brain = useRef(new CompanionBehavior())
  const context = useRef<CompanionContext>({musicPlaying:false,idleSeconds:0,mediaAvailable:false})
  const mode = useRef<BehaviorMode>('calm')
  const musicEnabled = useRef(true)
  const free = useRef(false)
  const speaking = useRef(false)
  const [revision, setRevision] = useState(0)
  const [problem, setProblem] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [clips, setClips] = useState<string[]>([])
  const [mapping, setMapping] = useState<MotionMap>({})
  const availableActions = useRef<string[]>(['builtin:stand'])
  availableActions.current = (['stand','fidget','wave','pose','sit','walk','dance','stretch','lie','sleep'] as Action[]).filter(action=>action==='stand'||!!assignedMotion(action,clips,mapping)).map(action=>`builtin:${action}`)
  const dragging=useRef(false)
  /** Курсор в долях окна: нужен и взгляду, и выбору занятия. */
  const cursor=useRef<{x:number;y:number}|undefined>(undefined)
  /** Когда последний раз откликались на касание: без паузы выходит дёрганье. */
  const touchedAt=useRef(0)
  /** Где нажали, пока не решено — перетаскивание это или касание. */
  const press=useRef<{x:number;y:number}|null>(null)
  /** Величины прочитаны из настроек, и когда их последний раз сохраняли. */
  const restored=useRef(false)
  const savedAt=useRef(0)
  const available=(action:Action)=>action==='stand'||!!assignedMotion(action,clips,mapping)
  /** Когда прыгала последний раз: прыжки каждые полминуты — это суета, а не жизнь. */
  const hoppedAt=useRef(performance.now()/1000)
  const hopping=useRef(false)

  useEffect(() => {
    const events = [
      listen<AvatarSignal>(AVATAR_EVENT, e => { latest.current = e.payload; sceneRef.current?.setState(e.payload.state) }),
      listen<string>(AVATAR_PLAY_EVENT, e => {
        if (e.payload === 'auto') {brain.current.resume(performance.now()/1000);return}
        brain.current.manual(e.payload, performance.now()/1000)
        if (!e.payload.startsWith('emotion:')) motion.current = motionFor(e.payload)
        sceneRef.current?.play(e.payload)
      }),
      listen<AvatarPose>(AVATAR_POSE_EVENT, e => sceneRef.current?.setPose(e.payload)),
      listen('yuki://avatar-reload', () => setRevision(v => v + 1)),
      listen('yuki://motion-settings',()=>{void settingGet('avatar.motionmap').then(value=>{const map=parseMotionMap(value);setMapping(map);sceneRef.current?.configureMotions(map)})}),
      listen('yuki://companion-placement', () => { void settingGet('avatar.window').then(v => { target.current = v ? Number(v) : null }) }),
    ]
    return () => { for (const p of events) void p.then(off => off()) }
  }, [])

  useEffect(() => {
    let disposed = false
    setLoading(true); setProblem(null)
    sceneRef.current?.dispose(); sceneRef.current = null
    void (async () => {
      try {
        const [status, clips, windowId] = await Promise.all([avatarStatus(), avatarAnimations(), settingGet('avatar.window')])
        target.current = windowId ? Number(windowId) : null
        free.current = status.anchor === 'free'
        if (disposed || !canvasRef.current) return
        const { createScene } = await import('./scene')
        if (disposed) return
        const scene = await createScene(canvasRef.current, status.model, status.pose, clips.map(c=>c.name))
        if (disposed) { scene.dispose(); return }
        sceneRef.current = scene
        const map=parseMotionMap(await settingGet('avatar.motionmap'))
        if(disposed)return
        setMapping(map);setClips(clips.map(c=>c.name));scene.configureMotions(map)
        scene.resize(canvasRef.current.clientWidth, canvasRef.current.clientHeight)
        scene.setState(latest.current.state)
        const savedAction = await settingGet('avatar.action')
        if (disposed) return
        const action = savedAction && savedAction !== 'auto' ? savedAction : 'builtin:wave'
        motion.current = motionFor(action)
        brain.current.manual(action, performance.now()/1000 - (savedAction ? 0 : 60))
        if (!savedAction || savedAction === 'auto' || savedAction === 'builtin:stand') brain.current.resume(performance.now()/1000 + 12)
        scene.play(action)
        setLoading(false)
      } catch (e) { if (!disposed) { setProblem(String(e)); setLoading(false) } }
    })()
    return () => { disposed = true; sceneRef.current?.dispose(); sceneRef.current = null }
  }, [revision])

  useEffect(() => {
    let disposed = false, busy = false
    const refresh = async () => {
      if (busy) return
      busy = true
      try {
        const [ctx,behavior,music,anchor] = await Promise.all([companionContext(),settingGet('avatar.behavior'),settingGet('avatar.music'),settingGet('avatar.anchor')])
        if (disposed) return
        context.current=ctx; mode.current=(behavior || 'calm') as BehaviorMode;musicEnabled.current=music!=='off';free.current=anchor==='free'

        // Внутренние величины переживают перезапуск: иначе персонаж каждое утро
        // просыпается одинаковым, и накопленная привязанность ничего не значит.
        if (!restored.current) {
          restored.current = true
          brain.current.restore(parseVitals(await settingGet('avatar.vitals')))
        } else if (performance.now() - savedAt.current > 120000) {
          savedAt.current = performance.now()
          void settingSet('avatar.vitals', JSON.stringify(brain.current.vitals)).catch(() => undefined)
        }
      } catch { /* Behavior still works when media sessions are unavailable. */ }
      finally {busy=false}
    }
    void refresh()
    const poll = setInterval(()=>void refresh(),3000)
    const tick = setInterval(() => {
      if (!sceneRef.current) return
      const input = {...context.current,mode:mode.current,musicEnabled:musicEnabled.current,canWalk:!free.current,availableActions:availableActions.current,state:speaking.current?'speaking':latest.current.state,passive:latest.current.passive,cursor:cursor.current}
      const action = brain.current.tick(performance.now()/1000, input)
      // Наружу для диагностики: почему компаньон выбрал именно это занятие,
      // по состоянию сцены не видно, а гадать об этом дорого.
      ;(window as unknown as {yukiBrain?:()=>unknown}).yukiBrain = () => ({
        action: brain.current.action, vitals: brain.current.vitals, decided: action,
        state: input.state, passive: input.passive, cursor: input.cursor,
        available: input.availableActions, mode: input.mode, canWalk: input.canWalk,
        idleSeconds: input.idleSeconds,
      })
      if (action) {sceneRef.current.play(action);motion.current=motionFor(sceneRef.current.activity())}
      void maybeHop(input.mode)
    }, 1000)
    return () => {disposed=true;clearInterval(poll);clearInterval(tick)}
  }, [])

  useEffect(() => {
    let disposed = false, busy = false, interactive = true
    const poll = setInterval(() => {
      if (busy || !sceneRef.current || dragging.current || document.hidden) return
      busy = true
      void Promise.all([
        voicePlayback().then(p => { if (!disposed) {speaking.current=p.speaking;sceneRef.current?.setSpeech(p.speaking, p.level)} }),
        companionMotionTick(sceneRef.current.contact(), sceneRef.current.activity() === 'builtin:walk', target.current).then(d => { if (!disposed) sceneRef.current?.setDirection(d) }),
        companionPointer(interactive).then(p => {if (!disposed && sceneRef.current) {interactive=sceneRef.current.hitTest(p.x,p.y);cursor.current=p;sceneRef.current.setPointer(p.x*2-1,1-p.y*2)}}).catch(()=>undefined),
      ]).catch(() => undefined).finally(() => { busy = false })
    }, 120)
    return () => { disposed = true; clearInterval(poll) }
  }, [])

  /**
   * Перепрыгнуть на другое окно или обратно на панель задач (Desktop Mate §14–16).
   *
   * Решает скука, а не таймер: заскучав, персонаж ищет, куда бы переместиться.
   * Не прыгает, пока его держат, пока он свободно стоит там, куда его поставили,
   * во время разговора и в тихом режиме.
   */
  const maybeHop = useCallback(async (mode: BehaviorMode) => {
    const scene = sceneRef.current
    const now = performance.now() / 1000
    if (!scene || hopping.current || dragging.current || free.current || mode === 'quiet') return
    if (latest.current.state !== 'idle' && !latest.current.passive) return
    const pause = mode === 'playful' ? 90 : 180
    if (now - hoppedAt.current < pause || brain.current.vitals.boredom < 0.3 || Math.random() > 0.25) return
    hopping.current = true
    hoppedAt.current = now
    try {
      const surfaces = (await companionSurfaces()).filter(s => s.id !== target.current)
      // С окна — то на другое окно, то домой на панель задач.
      const next = target.current !== null && (surfaces.length === 0 || Math.random() < 0.4)
        ? null
        : surfaces[Math.floor(Math.random() * surfaces.length)]?.id ?? null
      if (next === null && target.current === null) return
      if (clips.includes(JUMP_CLIP)) scene.play(`once:${JUMP_CLIP}`)
      target.current = next
      await companionAttach(next)
      // Приземлилась: на окне садится, на панели задач стоит.
      setTimeout(() => {
        const landing = next !== null && available('sit') ? 'builtin:sit' : 'builtin:stand'
        brain.current.manual(landing, performance.now() / 1000)
        sceneRef.current?.play(landing)
      }, 900)
    } catch {
      /* окна могли закрыться — прыгнем в следующий раз */
    } finally {
      hopping.current = false
    }
  }, [clips, mapping])

  /**
   * Отклик на касание.
   *
   * Одно прикосновение поднимает сразу несколько каналов: выражение лица,
   * жест, взгляд и внутреннее состояние. Именно согласованность каналов
   * читается как характер; одна только анимация выглядит как нажатие кнопки.
   *
   * По голове и по телу отклик разный, и оба мягче, если гладить подряд:
   * повторять «ура» на каждый клик — это не радость, а тик.
   */
  const touch = useCallback((zone: 'head' | 'body') => {
    const scene = sceneRef.current
    if (!scene) return
    const now = performance.now() / 1000
    const fresh = now - touchedAt.current > 1.2
    touchedAt.current = now

    brain.current.touched(zone === 'head' ? 0.06 : 0.03)
    // Выражение подбирается под модель: «удивления» у многих авторов нет,
    // и просьба показать его молча не делает ничего.
    const wanted = zone === 'head' ? ['happy', 'relaxed'] : ['surprised', 'happy', 'relaxed']
    const face = wanted.find(name => scene.hasExpression(name))
    if (face) scene.play(`emotion:${face}`)
    if (!fresh) return

    // Жест только на нечастые касания и только если клип для него есть.
    const gesture = zone === 'head' ? 'builtin:wave' : 'builtin:fidget'
    if (available(gesture.slice(8) as Action)) {
      brain.current.manual(gesture, now)
      scene.play(gesture)
    }
  }, [clips, mapping])

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const observer = new ResizeObserver(() => sceneRef.current?.resize(canvas.clientWidth, canvas.clientHeight))
    observer.observe(canvas)
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined
    const remember = () => { clearTimeout(timer); timer = setTimeout(() => void avatarRememberPlacement().catch(() => undefined), 700) }
    const win = getCurrentWindow()
    const events = [win.onMoved(remember), win.onResized(remember)]
    return () => { clearTimeout(timer); for (const p of events) void p.then(off => off()) }
  }, [])

  return <div className="avatar">
    {/*
      Нажатие — это либо перетаскивание, либо касание, и различаются они
      движением, а не отдельными кнопками.

      Раньше перетаскивание начиналось прямо в `pointerdown`: система забирала
      мышь себе, события `click` не возникало вовсе, и потрогать персонажа было
      физически нельзя. Теперь окно едет за мышью только после того, как мышь
      действительно поехала, а нажатие без движения достаётся персонажу.
    */}
    <canvas ref={canvasRef} className="avatar__canvas"
      onPointerDown={e=>{if(e.button!==0)return;press.current={x:e.clientX,y:e.clientY}}}
      onPointerMove={e=>{
        const start=press.current
        if(!start||dragging.current)return
        if(Math.hypot(e.clientX-start.x,e.clientY-start.y)<DRAG_SLOP)return
        press.current=null
        dragging.current=true
        free.current=true
        // Персонаж встаёт: ехать в окне сидя или танцуя он не должен.
        brain.current.manual('builtin:stand',performance.now()/1000)
        sceneRef.current?.play('builtin:stand')
        // Подняли — удивилась, поставили — обрадовалась вниманию (VPet §7,
        // разбор Desktop Mate §19): перенос это тоже взаимодействие, а не
        // перемещение окна с картинкой.
        const face=(names:string[])=>{const scene=sceneRef.current;const found=names.find(name=>scene?.hasExpression(name));if(found)scene?.play(`emotion:${found}`)}
        face(['surprised','happy'])
        void avatarSetAnchor('free').then(()=>getCurrentWindow().startDragging()).catch(err=>setProblem(String(err))).finally(()=>{
          dragging.current=false
          brain.current.touched(0.03)
          face(['happy','relaxed'])
        })
      }}
      onPointerUp={e=>{
        const start=press.current
        press.current=null
        if(!start||dragging.current||!canvasRef.current)return
        const box=canvasRef.current.getBoundingClientRect()
        const zone=sceneRef.current?.touchZone((e.clientX-box.left)/box.width,(e.clientY-box.top)/box.height)
        if(zone)touch(zone)
      }}
      onPointerCancel={()=>{press.current=null}}
    />
    <div className="avatar__controls">
      <button title="Помахать" disabled={!available('wave')} onClick={() => void avatarPlay('builtin:wave')}>✋</button>
      <button title="Сесть" disabled={!available('sit')} onClick={() => void avatarPlay('builtin:sit')}>↓</button>
      <button title="Встать" onClick={() => void avatarPlay('builtin:stand')}>↑</button>
      <button title="Прогуляться" disabled={!available('walk')} onClick={() => void avatarPlay('builtin:walk')}>↔</button>
      <button title="Танцевать" disabled={!available('dance')} onClick={() => void avatarPlay('builtin:dance')}>♫</button>
      <button title="Лечь" disabled={!available('lie')} onClick={() => void avatarPlay('builtin:lie')}>☾</button>
      <button title="Заниматься своими делами" onClick={() => void avatarPlay('auto')}>✦</button>
      <button title="Скрыть компаньона" onClick={() => void avatarClose()}>×</button>
    </div>
    {(problem || loading) && <div className="avatar__fallback"><div className="avatar__orb" /><p className="avatar__note">{problem || 'Знакомлюсь с персонажем…'}</p></div>}
  </div>
}

/** Авторский клип прыжка из библиотеки: скелет прыгает им, окно летит по дуге. */
const JUMP_CLIP = 'Arisa__NewJump'

function motionFor(action: string) {
  if (!action || action === 'builtin:stand' || action === 'builtin:wave' || action.startsWith('emotion:')) return 'stand'
  return action.replace('builtin:', '')
}
