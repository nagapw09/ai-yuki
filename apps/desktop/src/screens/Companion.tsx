import { useCallback, useEffect, useRef, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { emit } from '@tauri-apps/api/event'
import {
  avatarStatus, avatarOpen, avatarClose, avatarPlay, avatarSetPose, avatarSetAnchor,
  avatarSetClickThrough, avatarAnimations, avatarExpressionNames, profileList, profileApply,
  profileSave, personaName, personaSetName, personaGet, personaSet, settingGet, settingSet,
  companionImport, companionAttach, listWindows, voiceSpeak, voiceStatus, voiceSetVoice,
  voicePlayback, type AvatarStatus, type Profile, type AnimationClip, type WindowInfo,
} from '../bridge'
import { useUiStore } from '../state/store'
import type { AvatarScene } from '../avatar/scene'
import { assignedMotion,parseMotionMap,motionLabel,type Action,type MotionMap } from '../avatar/motions'
import { Avatar } from './Settings'
import './Companion.css'

const ACTIONS = [['fidget','Размяться'],['wave','Помахать'],['pose','Позировать'],['sit','Сесть'],['stand','Встать'],['walk','Прогуляться'],['dance','Танцевать'],['stretch','Потянуться'],['lie','Лечь'],['sleep','Поспать']] as const
const EXPRESSIONS = [['happy','Улыбка'],['surprised','Удивление'],['sad','Грусть'],['relaxed','Спокойствие']] as const

export function Companion() {
  const [profiles, setProfiles] = useState<Profile[]>([])
  const [status, setStatus] = useState<AvatarStatus | null>(null)
  const [name, setName] = useState('')
  const [character, setCharacter] = useState('')
  const [behavior, setBehavior] = useState('calm')
  const [quality, setQuality] = useState('original')
  const [music, setMusic] = useState(true)
  const [voice, setVoice] = useState('')
  const [voices, setVoices] = useState<string[]>([])
  const [clips, setClips] = useState<AnimationClip[]>([])
  const [expressions, setExpressions] = useState<string[]>([])
  const [windows, setWindows] = useState<WindowInfo[]>([])
  const [selectedWindow, setSelectedWindow] = useState('')
  const [busy, setBusy] = useState(false)
  const [note, setNote] = useState('')
  const [mapping,setMapping]=useState<MotionMap>({})
  const [motionQuery,setMotionQuery]=useState('')
  const [playMode,setPlayMode]=useState('auto')
  const [selectedClip,setSelectedClip]=useState('')
  const [assignAction,setAssignAction]=useState<Action>('dance')
  const preview = useRef<AvatarScene | null>(null)
  const [tab, setTab] = useState<typeof TABS[number][0]>('look')
  const active = profiles.find(p => p.active)

  const reload = useCallback(async () => {
    const [list, avatar, savedName, persona, moves, speech, mode, selectedVoice] = await Promise.all([
      profileList(), avatarStatus(), personaName(), personaGet(), avatarAnimations(), voiceStatus(), settingGet('avatar.behavior'), settingGet('voice.tts.voice'),
    ])
    setProfiles(list); setStatus(avatar); setName(savedName); setCharacter(persona.custom)
    setClips(moves); setVoices(speech.voices); setBehavior(mode || 'calm'); setVoice(selectedVoice || '')
    setMusic(await settingGet('avatar.music') !== 'off')
    setQuality(await settingGet('avatar.quality') || 'original')
    setMapping(parseMotionMap(await settingGet('avatar.motionmap')))
    setExpressions((await avatarExpressionNames()).filter(name=>!['aa','ih','ou','ee','oh','lookUp','lookDown','lookLeft','lookRight'].includes(name)))
    useUiStore.getState().setAssistantName(savedName)
  }, [])
  useEffect(() => { void reload().catch(e => setNote(String(e))) }, [reload])

  const run = async (task: () => Promise<void>, refresh = false) => {
    setBusy(true); setNote('')
    try { await task(); if (refresh) await reload(); else setStatus(await avatarStatus()) } catch (e) { setNote(String(e)) } finally { setBusy(false) }
  }
  const importAssets = () => run(async () => {
    const selected = await open({ multiple: true, filters: [{ name: 'Модели и движения', extensions: ['vrm','vrma','zip'] }] })
    if (!selected) return
    const result = await companionImport(Array.isArray(selected) ? selected : [selected])
    setNote(`Добавлено персонажей: ${result.models}, движений: ${result.animations}.${result.unsupported.length ? ` Форматы Unity/FBX требуют конвертации: ${result.unsupported.slice(0,5).join(', ')}.` : ''}`)
    const list = await profileList()
    if (!list.some(p => p.active) && list[0]) await profileApply(list[0].id)
  }, true)
  const perform = (action: string) => run(async () => {
    if (status?.open) {await avatarSetPose('full');await avatarPlay(action)}
    else if(preview.current) preview.current.play(action)
  })
  const save = () => run(async () => {
    await personaSetName(name)
    const current = await personaGet()
    await personaSet({ ...current, role: character.trim() ? 'custom' : 'assistant', custom: character })
    await settingSet('avatar.behavior', behavior)
    await settingSet('avatar.music', music ? 'on' : 'off')
    await voiceSetVoice(voice)
    await profileSave(active?.name || name || 'Мой компаньон')
    setNote('Имя, характер, голос и облик сохранены в профиле.')
  }, true)

  const clipNames = clips.map(c => c.name)
  const toggle = (on: boolean, label: string, change: (value: boolean) => void) =>
    <button type="button" role="switch" aria-checked={on} aria-label={label} className="companion-switch" data-on={on} disabled={busy} onClick={() => change(!on)} />

  // Раскладка по макету: слева персонаж и где он стоит, справа вкладки. Раньше
  // всё это было одной лентой на три экрана вниз, и нужное приходилось искать.
  return <div className="companion-page">
    <section className="companion-stage">
      <div className="companion-stage__view">
        {status?.modelPresent
          ? status.open
            ? <div className="companion-empty"><span>✧</span><p>Персонаж на рабочем столе</p><small>Движения и эмоции проигрываются на нём.</small></div>
            : <Preview key={quality} model={status.model} sceneRef={preview} />
          : <div className="companion-empty"><span>✧</span><p>Добавьте персонажа в формате VRM</p><small>Можно выбрать несколько моделей или ZIP-архив.</small></div>}
        {status?.open && <span className="companion-badge">на рабочем столе</span>}
      </div>
      <div className="companion-stage__bottom">
        <div className="companion-segment" role="group" aria-label="Где стоит персонаж">
          <button type="button" data-active={status?.anchor !== 'free' && !selectedWindow} disabled={busy} onClick={() => void run(async () => { setSelectedWindow(''); await companionAttach(null) })}>Панель задач</button>
          <button type="button" data-active={!!selectedWindow} disabled={busy} onClick={() => void listWindows().then(items => setWindows(items.filter(w => !w.isMinimized && w.title && w.appName !== 'yuki-desktop.exe'))).catch(e => setNote(String(e)))}>На окне</button>
          <button type="button" data-active={status?.anchor === 'free'} disabled={busy} onClick={() => void run(async () => { setSelectedWindow(''); await avatarSetAnchor('free') })}>Свободно</button>
        </div>
        {!!windows.length && <div className="companion-fields">
          <select aria-label="Окно для персонажа" className="settings__input" value={selectedWindow} onChange={e => setSelectedWindow(e.target.value)}><option value="">Окно, на которое сесть</option>{windows.map(w => <option key={w.id} value={w.id}>{w.title}</option>)}</select>
          <button className="settings__button" disabled={!selectedWindow || busy} onClick={() => void run(async () => { await companionAttach(Number(selectedWindow)); if (!status?.open) await avatarOpen(); await avatarSetPose('full'); await avatarPlay('builtin:sit'); setWindows([]) })}>Сесть</button>
        </div>}
        <button className="companion-primary" disabled={busy || !status?.modelPresent} onClick={() => void run(async () => { if (status?.open) await avatarClose(); else { await avatarSetPose('full'); await avatarOpen() } })}>{status?.open ? 'Скрыть с рабочего стола' : 'Позвать на рабочий стол'}</button>
      </div>
    </section>

    <div className="companion-side">
      <div className="companion-tabs-row">
        <div className="companion-segment companion-segment--tabs" role="tablist">
          {TABS.map(([id, label]) => <button key={id} type="button" role="tab" aria-selected={tab === id} data-active={tab === id} onClick={() => setTab(id)}>{label}</button>)}
        </div>
        <button className="settings__button" disabled={busy} onClick={() => void importAssets()}>＋ Добавить VRM</button>
      </div>
      {note && <p className="companion-note" role="status">{note}</p>}

      <div className="companion-scroll">
        {tab === 'look' && <>
          <div className="companion-models">
            {profiles.map(p => <button key={p.id} type="button" data-active={p.active} disabled={busy} onClick={() => void run(async () => { await profileApply(p.id) }, true)}>
              <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><circle cx="12" cy="7" r="3.5" /><path d="M5 21c.8-4 3.6-6.5 7-6.5s6.2 2.5 7 6.5" /></svg>
              <span>{p.name}</span>
            </button>)}
            {!profiles.length && <p className="companion-hint">Персонажей пока нет — добавьте файл VRM.</p>}
          </div>
          <div className="companion-card">
            <div className="companion-row"><div><b>Поведение</b><small>Как часто сама меняет занятие</small></div>
              <div className="companion-segment">{BEHAVIORS.map(([id, label]) => <button key={id} type="button" data-active={behavior === id} disabled={busy} onClick={() => void run(async () => { setBehavior(id); await settingSet('avatar.behavior', id) })}>{label}</button>)}</div></div>
            <div className="companion-row"><div><b>Танцевать под музыку</b><small>Когда в браузере или плеере играет трек</small></div>
              {toggle(music, 'Танцевать под музыку', value => void run(async () => { setMusic(value); await settingSet('avatar.music', value ? 'on' : 'off') }))}</div>
            <div className="companion-row"><div><b>Пропускать клики сквозь персонажа</b><small>Не мешает работать с окнами под ним</small></div>
              {toggle(status?.clickThrough ?? false, 'Пропускать клики', value => void run(async () => { await avatarSetClickThrough(value) }))}</div>
            <div className="companion-row"><div><b>Качество изображения</b><small>Экономный режим уменьшает текстуры и память</small></div>
              <select className="settings__input companion-select" value={quality} disabled={busy} onChange={e => { const value = e.target.value; void run(async () => { await settingSet('avatar.quality', value); setQuality(value); await emit('yuki://avatar-reload') }) }}><option value="original">Оригинал</option><option value="economy">Экономия</option></select></div>
          </div>
          <div className="companion-actions-row"><span>Попросить:</span>{ACTIONS.filter(([id]) => id !== 'stand').slice(0, 6).map(([id, label]) => <button className="settings__button" key={id} disabled={busy || !status?.modelPresent || !assignedMotion(id, clipNames, mapping)} onClick={() => void perform(`builtin:${id}`)}>{label}</button>)}<button className="settings__button" disabled={busy || !status?.open} onClick={() => void perform('auto')}>Сама</button></div>
        </>}

        {tab === 'moves' && <>
          <div className="companion-card companion-card--pad">
            <b className="companion-card__title">Действия</b>
            <div className="companion-buttons">{ACTIONS.map(([id, label]) => <button className="settings__button" key={id} title={assignedMotion(id, clipNames, mapping) || 'Назначьте готовое движение в библиотеке'} disabled={busy || !status?.modelPresent || (id !== 'stand' && !assignedMotion(id, clipNames, mapping))} onClick={() => void perform(`builtin:${id}`)}>{label}</button>)}</div>
            <b className="companion-card__title">Эмоции</b>
            <div className="companion-buttons">{EXPRESSIONS.map(([id, label]) => <button className="companion-expression" key={id} disabled={busy || !status?.modelPresent} onClick={() => void perform(`emotion:${id}`)}>{label}</button>)}</div>
            <details><summary>Выражения этой модели · {expressions.length}</summary><div className="companion-buttons">{expressions.map(name => <button className="companion-expression" key={name} disabled={busy} onClick={() => void perform(`emotion:${name}`)}>{name}</button>)}</div></details>
          </div>
          <div className="companion-card companion-card--pad">
            <b className="companion-card__title">Библиотека движений · {clips.length}</b>
            <input className="settings__input" aria-label="Поиск движений" placeholder="Найти позу или танец…" value={motionQuery} onChange={e => setMotionQuery(e.target.value)} />
            <div className="motion-library">{clips.filter(c => c.name.toLowerCase().includes(motionQuery.toLowerCase())).map(c => <button key={c.name} data-selected={selectedClip === c.name} onClick={() => { setSelectedClip(c.name); void perform(`${playMode}:${c.name}`) }}><span>▷</span><strong>{motionLabel(c.name)}</strong><small>{c.name.includes('__') ? c.name.split('__')[0] : 'VRMA Motion Pack'}</small></button>)}</div>
            <div className="companion-fields">
              <select className="settings__input" aria-label="Как проигрывать" value={playMode} onChange={e => { setPlayMode(e.target.value); if (selectedClip) void perform(`${e.target.value}:${selectedClip}`) }}><option value="auto">Автоматически</option><option value="once">Один раз</option><option value="loop">Повторять</option><option value="hold">Удерживать позу</option></select>
              <select className="settings__input" aria-label="Назначить действию" value={assignAction} onChange={e => setAssignAction(e.target.value as Action)}>{ACTIONS.map(([id, label]) => <option key={id} value={id}>{label}</option>)}</select>
              <button className="settings__button" disabled={!selectedClip} onClick={() => void run(async () => { const next = { ...mapping, [assignAction]: selectedClip }; await settingSet('avatar.motionmap', JSON.stringify(next)); setMapping(next); preview.current?.configureMotions(next); await emit('yuki://motion-settings'); setNote(`«${motionLabel(selectedClip)}» назначено: ${ACTIONS.find(a => a[0] === assignAction)?.[1]}`) })}>Назначить</button>
              <button className="settings__button" onClick={() => void perform('builtin:stand')}>Стоп</button>
            </div>
            <p className="companion-hint">Без своего назначения действие берёт случайный клип из подходящего набора — так движения не повторяются.</p>
          </div>
        </>}

        {tab === 'character' && <div className="companion-card companion-card--pad">
          <div className="companion-fields"><label className="setup-field">Имя<input className="settings__input" value={name} maxLength={32} onChange={e => setName(e.target.value)} /></label></div>
          <label className="setup-field">Характер<textarea className="settings__input" rows={3} value={character} onChange={e => setCharacter(e.target.value)} placeholder="Например: дружелюбная, с лёгким юмором, отвечает коротко" /></label>
          <label className="setup-field">Голос<select className="settings__input" value={voice} onChange={e => setVoice(e.target.value)}><option value="">Системный по умолчанию</option>{voices.map(v => <option key={v}>{v}</option>)}</select></label>
          <div className="companion-buttons"><button className="settings__button" disabled={busy || !active} onClick={() => void save()}>Сохранить профиль</button><button className="settings__button" disabled={busy} onClick={() => void run(async () => { if (voice) await voiceSetVoice(voice); await voiceSpeak(`Привет! Я ${name || 'Юки'}. Рада тебя видеть.`) })}>Послушать голос</button></div>
          <p className="companion-hint">Профиль хранит облик, имя, характер и голос вместе — переключается одним нажатием на вкладке «Облик».</p>
        </div>}

        {tab === 'window' && <div className="companion-embed"><Avatar /></div>}
      </div>
    </div>
  </div>
}

const TABS = [['look', 'Облик'], ['moves', 'Движения'], ['character', 'Характер и голос'], ['window', 'Окно']] as const
const BEHAVIORS = [['calm', 'Спокойное'], ['playful', 'Живое'], ['quiet', 'Тихое']] as const

function Preview({model, sceneRef}: {model: string; sceneRef: React.MutableRefObject<AvatarScene | null>}) {
  const canvas = useRef<HTMLCanvasElement>(null)
  const [error,setError] = useState('')
  const [loading,setLoading] = useState(true)
  useEffect(() => {
    let dead = false
    setLoading(true); setError('')
    void (async () => {
      try {
        const { createScene } = await import('../avatar/scene')
        if (dead || !canvas.current) return
        const clips = await avatarAnimations()
        if (dead) return
        const scene = await createScene(canvas.current, model, 'full', clips.map(c=>c.name))
        if (dead) { scene.dispose(); return }
        sceneRef.current = scene
        scene.configureMotions(parseMotionMap(await settingGet('avatar.motionmap')))
        scene.resize(canvas.current.clientWidth,canvas.current.clientHeight)
        scene.play('builtin:wave'); setLoading(false)
      } catch(e) { if(!dead) { setError(String(e));setLoading(false) } }
    })()
    const resize = new ResizeObserver(() => { if(canvas.current) sceneRef.current?.resize(canvas.current.clientWidth,canvas.current.clientHeight) })
    if(canvas.current) resize.observe(canvas.current)
    const poll = setInterval(() => { if(sceneRef.current && !document.hidden && !document.documentElement.dataset.paused) void voicePlayback().then(p=>sceneRef.current?.setSpeech(p.speaking,p.level)).catch(()=>undefined) },120)
    return () => { dead=true;clearInterval(poll);resize.disconnect();sceneRef.current?.dispose();sceneRef.current=null }
  }, [model, sceneRef])
  return <div className="companion-preview"><canvas ref={canvas} onClick={()=>sceneRef.current?.play('emotion:happy')} />{(loading||error) && <p className="companion-preview__message">{error || 'Загружаю персонажа…'}</p>}</div>
}
