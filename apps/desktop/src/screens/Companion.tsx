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

  return <div className="companion-page">
    <header className="companion-heading"><div><span>ВАШ КОМПАНЬОН</span><h1>Кто сегодня рядом?</h1><p>Облик, голос и характер — в одном профиле.</p></div><button className="settings__button" disabled={busy} onClick={() => void importAssets()}>＋ Добавить VRM / ZIP</button></header>
    {note && <p className="companion-note" role="status">{note}</p>}
    <div className="companion-layout">
      <section className="companion-stage">
        <div className="companion-stage__top"><span>{name || 'Ваш персонаж'}</span><small>{status?.open ? 'На рабочем столе' : 'Предпросмотр'}</small></div>
        {status?.modelPresent ? status.open ? <div className="companion-empty"><span>✧</span><p>Компаньон на рабочем столе</p><small>Выбранные движения проигрываются на нём.</small><button className="settings__button" onClick={()=>void run(async()=>{await avatarClose()})}>Показывать здесь</button></div> : <Preview key={quality} model={status.model} sceneRef={preview} /> : <div className="companion-empty"><span>✧</span><p>Добавьте персонажа в формате VRM</p><small>Можно выбрать несколько моделей или ZIP-архив.</small></div>}
        <div className="companion-stage__bottom"><button className="companion-primary" disabled={busy || !status?.modelPresent} onClick={() => void run(async () => { if (status?.open) await avatarClose(); else { await avatarSetPose('full'); await avatarOpen() } })}>{status?.open ? 'Скрыть с рабочего стола' : 'Позвать компаньона'}</button></div>
      </section>
      <div className="companion-controls">
        <section><h2>Качество изображения</h2><select className="settings__input" value={quality} disabled={busy} onChange={e=>{const value=e.target.value;void run(async()=>{await settingSet('avatar.quality',value);setQuality(value);await emit('yuki://avatar-reload')})}}><option value="original">Оригинальные текстуры · чёткое изображение</option><option value="economy">Экономия памяти · уменьшенные текстуры</option></select><p className="companion-hint">По умолчанию сохраняется качество исходной модели. Экономный режим уменьшает детали.</p></section>
        <section><h2>Персонаж</h2><label className="setup-field">Быстрая смена<select className="settings__input" value={active?.id || ''} disabled={busy} onChange={e => void run(async () => { await profileApply(e.target.value) }, true)}><option value="" disabled>Выберите персонажа</option>{profiles.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}</select></label>
          <div className="companion-fields"><label className="setup-field">Имя<input className="settings__input" value={name} maxLength={32} onChange={e => setName(e.target.value)} /></label><label className="setup-field">Поведение<select className="settings__input" value={behavior} onChange={e => setBehavior(e.target.value)}><option value="calm">Спокойное</option><option value="playful">Живое</option><option value="quiet">Без случайных жестов</option></select></label></div>
          <label className="setup-field">Характер<textarea className="settings__input" rows={3} value={character} onChange={e => setCharacter(e.target.value)} placeholder="Например: дружелюбная, с лёгким юмором, отвечает коротко" /></label>
          <label className="setup-field">Голос<select className="settings__input" value={voice} onChange={e => setVoice(e.target.value)}><option value="">Системный по умолчанию</option>{voices.map(v => <option key={v}>{v}</option>)}</select></label>
          <label className="companion-hint"><input type="checkbox" checked={music} onChange={e=>setMusic(e.target.checked)} /> Танцевать, когда музыкальный плеер играет</label>
          <p className="companion-hint">В спокойном и живом режимах компаньон сам меняет занятия. После вашей команды выбранное действие сохраняется на две минуты. Во время разговора он отвлекается на вас.</p>
          <div className="companion-buttons"><button className="settings__button" disabled={busy || !active} onClick={() => void save()}>Сохранить профиль</button><button className="settings__button" disabled={busy} onClick={() => void run(async () => { if (voice) await voiceSetVoice(voice); await voiceSpeak(`Привет! Я ${name || 'Юки'}. Рада тебя видеть.`) })}>Послушать голос</button></div>
        </section>
        <section><h2>Движения и эмоции</h2><div className="companion-buttons">{ACTIONS.map(([id,label]) => <button className="settings__button" key={id} title={assignedMotion(id,clips.map(c=>c.name),mapping)||'Назначьте готовое движение в библиотеке'} disabled={busy || !status?.modelPresent || (id!=='stand'&&!assignedMotion(id,clips.map(c=>c.name),mapping))} onClick={() => void perform(`builtin:${id}`)}>{label}</button>)}</div><div className="companion-buttons">{EXPRESSIONS.map(([id,label]) => <button className="companion-expression" key={id} disabled={busy || !status?.modelPresent} onClick={() => void perform(`emotion:${id}`)}>{label}</button>)}</div>
          <h3>Библиотека движений · {clips.length}</h3>
          <details><summary>Выражения из выбранной модели · {expressions.length}</summary><div className="companion-buttons">{expressions.map(name=><button className="companion-expression" key={name} disabled={busy} onClick={()=>void perform(`emotion:${name}`)}>{name}</button>)}</div><p className="companion-hint">Это готовые выражения автора VRM. Скачанные Unity-клипы для другого лица требуют соответствующей модели.</p></details>
          <input className="settings__input" aria-label="Поиск движений" placeholder="Найти позу или танец…" value={motionQuery} onChange={e=>setMotionQuery(e.target.value)}/>
          <div className="motion-library">{clips.filter(c=>c.name.toLowerCase().includes(motionQuery.toLowerCase())).map(c=><button key={c.name} data-selected={selectedClip===c.name} onClick={()=>{setSelectedClip(c.name);void perform(`${playMode}:${c.name}`)}}><span>▷</span><strong>{motionLabel(c.name)}</strong><small>{c.name.includes('__')?c.name.split('__')[0]:'VRMA Motion Pack'}</small></button>)}</div>
          <div className="companion-fields"><label className="setup-field">Воспроизведение<select className="settings__input" value={playMode} onChange={e=>{setPlayMode(e.target.value);if(selectedClip)void perform(`${e.target.value}:${selectedClip}`)}}><option value="auto">Автоматически · позу удержать, движение проиграть</option><option value="once">Один раз и вернуться в покой</option><option value="loop">Повторять</option><option value="hold">Удерживать конечную позу</option></select></label><button className="settings__button" onClick={()=>void perform('builtin:stand')}>Остановить</button></div>
          <div className="companion-fields"><label className="setup-field">Назначить выбранное движение<select className="settings__input" value={assignAction} onChange={e=>setAssignAction(e.target.value as Action)}>{ACTIONS.map(([id,label])=><option key={id} value={id}>{label}</option>)}</select></label><button className="settings__button" disabled={!selectedClip} onClick={()=>void run(async()=>{const next={...mapping,[assignAction]:selectedClip};await settingSet('avatar.motionmap',JSON.stringify(next));setMapping(next);preview.current?.configureMotions(next);await emit('yuki://motion-settings');setNote(`«${motionLabel(selectedClip)}» назначено: ${ACTIONS.find(a=>a[0]===assignAction)?.[1]}`)})}>Назначить</button></div>
          <p className="companion-hint">Для действия без назначенного клипа кнопка выключена. Движения проигрываются из файлов; обратного воспроизведения и самодельного танца больше нет. Позы записаны одним кадром, поэтому в автоматическом режиме они удерживаются, а не мелькают.</p>
          <button className="settings__button" disabled={busy || !status?.open} onClick={()=>void perform('auto')}>Занимайся своими делами</button>
          <p className="companion-hint">Двойной щелчок по персонажу вызывает улыбку. Наведите курсор, чтобы открыть быстрые действия. Для свободной прогулки выберите панель задач или край окна.</p>
        </section>
        <section><h2>Место на экране</h2><div className="companion-buttons"><button className="settings__button" disabled={busy} onClick={() => void run(async () => { await companionAttach(null) })}>На панели задач</button><button className="settings__button" disabled={busy} onClick={() => void run(async () => { await avatarSetAnchor('free') })}>Свободно перемещать</button><button className="settings__button" onClick={() => void listWindows().then(items => setWindows(items.filter(w => !w.isMinimized && w.title && w.appName !== 'yuki-desktop.exe'))).catch(e => setNote(String(e)))}>Выбрать окно</button></div>
          {!!windows.length && <div className="companion-fields"><select aria-label="Окно для компаньона" className="settings__input" value={selectedWindow} onChange={e => setSelectedWindow(e.target.value)}><option value="">Окно, на которое сесть</option>{windows.map(w => <option key={w.id} value={w.id}>{w.title}</option>)}</select><button className="settings__button" disabled={!selectedWindow || busy} onClick={() => void run(async () => { await companionAttach(Number(selectedWindow)); if (!status?.open) await avatarOpen(); await avatarSetPose('full'); await avatarPlay('builtin:sit') })}>Сесть на окно</button></div>}
          <p className="companion-hint">Персонаж следует за верхним краем выбранного окна. Оставьте над окном место для фигуры.</p>
          <label className="companion-hint"><input type="checkbox" checked={status?.clickThrough ?? false} onChange={e => void run(async () => { await avatarSetClickThrough(e.target.checked) })} /> Пропускать клики сквозь персонажа</label>
        </section>
      </div>
    </div>
  </div>
}

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
