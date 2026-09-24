/**
 * Главный экран.
 *
 * Композиция повторяет раскладку Astra, на которую указал пользователь: слева
 * погода и курс, в центре имя, подзаголовок и две кнопки, справа три показателя
 * машины, под центром — что сейчас играет, в нижнем углу — быстрые значки.
 *
 * Здесь намеренно нет ни шара, ни строки ввода, ни списка дисков. Разговор
 * живёт в чате, а главный экран отвечает на один вопрос: что сейчас вокруг.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  isTauri, mediaControl, mediaNowPlaying, providerList, ratesGet, reminderCreate, reminderDelete,
  reminderList, settingGet, settingSet, wakeStatus, weatherGet,
  type NowPlaying, type ProviderRecord, type Rates, type Reminder, type Weather,
} from '../bridge'
import { useUiStore } from '../state/store'
import './Orbital.css'

export interface OrbitalProps { onSubmit: (text: string) => void; onToggleVoice: () => void }
interface Metrics { cpu:number|null; gpu:number|null; gpuName:string|null; gpus:{name:string;usage:number}[]; memoryUsed:number; memoryTotal:number; netDown?:number; netUp?:number; battery?:{percent:number;charging:boolean;secondsLeft:number|null}|null }
/** Байт в секунду → «1,2 МБ/с», «80 КБ/с». */
export const speed=(n:number)=>n>=1048576?`${(n/1048576).toFixed(1).replace('.',',')} МБ/с`:`${Math.round(n/1024)} КБ/с`
/** Остаток батареи: «2 ч 10 мин», «45 мин». */
export const timeLeft=(s:number)=>{const h=Math.floor(s/3600),m=Math.round(s%3600/60);return h?`${h} ч ${m} мин`:`${m} мин`}
const gb=(n:number)=>(n/1073741824).toFixed(1)
/** «NVIDIA GeForce RTX 4060 Laptop GPU» → «RTX 4060». */
const shortGpu=(name:string)=>name.replace(/\((R|TM)\)/g,'').replace(/\b(NVIDIA|Intel|AMD|GeForce|Graphics|Laptop|GPU)\b/g,'').replace(/\s+/g,' ').trim()||name
const clock=(s:number)=>`${Math.floor(s/60)}:${String(Math.floor(s%60)).padStart(2,'0')}`

export function Orbital({ onToggleVoice }: OrbitalProps) {
  const {voiceActive,headline,assistantName,setScreen}=useUiStore()
  const [metrics,setMetrics]=useState<Metrics|null>(null)
  const [providers,setProviders]=useState<ProviderRecord[]>([])
  const [error,setError]=useState('')

  const [wakeMissing,setWakeMissing]=useState(false)
  useEffect(()=>{if(isTauri())void providerList().then(setProviders).catch(()=>undefined)},[])
  // Без записанного обращения Yuki отзывается и на чужой разговор — это главная
  // причина ложных срабатываний, поэтому подсказка стоит прямо на главной.
  useEffect(()=>{if(isTauri())void wakeStatus().then(w=>setWakeMissing(!w.enrolled||!!w.legacy)).catch(()=>undefined)},[])
  useEffect(()=>{
    let dead=false,busy=false
    async function refresh(){if(!isTauri()||busy||document.hidden||document.documentElement.dataset.paused)return;busy=true;try{const data=await invoke<Metrics>('system_metrics');if(!dead)setMetrics(data)}catch{/* счётчики подождут следующей секунды */}finally{busy=false}}
    void refresh();const timer=setInterval(()=>void refresh(),1000);return()=>{dead=true;clearInterval(timer)}
  },[])

  const active=providers.find(p=>p.isDefault&&p.enabled)
  // Показываем каждый адаптер: на ноутбуке их два, и одно число без имени не с
  // чем сравнить в диспетчере задач.
  const gpus=metrics?.gpus??[]
  const state=headline||(voiceActive?'Скажите «Юки» — я слушаю':'Ваш голосовой помощник на рабочем столе')

  return <div className="home">
    <aside className="home__side">
      <WeatherBlock/>
      <RatesBlock/>
    </aside>

    <section className="home__center">
      <h1 className="home__mark">{assistantName||'Yuki'}<i aria-hidden="true">✦</i></h1>
      <p className="home__tagline" data-live={voiceActive||undefined}>{voiceActive&&<i aria-hidden="true"/>}{state}</p>
      <div className="home__actions">
        <button onClick={()=>setScreen('chat')}><ChatIcon/>Чат</button>
        <button className={voiceActive?'home__primary':undefined} onClick={onToggleVoice}><MicIcon/>{voiceActive?'Стоп':'Запуск'}</button>
      </div>
      <NowPlayingCard/>
      {!active&&<button className="home__hint" onClick={()=>setScreen('settings')}>Модель не подключена — открыть настройки →</button>}
      {error&&<p className="home__error" role="alert">{error}</p>}
    </section>

    <aside className="home__stats" aria-label="Загрузка компьютера">
      <Gauge label="CPU" value={metrics?.cpu??null}/>
      <Gauge label="RAM" value={metrics?.memoryTotal?metrics.memoryUsed/metrics.memoryTotal*100:null} note={metrics?`${gb(metrics.memoryUsed)} ГБ`:undefined}/>
      {gpus.length
        ?gpus.map((g,i)=><Gauge key={`${i}-${g.name}`} label="GPU" value={g.usage} note={shortGpu(g.name)} title={g.name}/>)
        :<Gauge label="GPU" value={metrics?.gpu??null} note={metrics?.gpuName?shortGpu(metrics.gpuName):undefined}/>}
      {metrics?.battery&&<Gauge label="Батарея" value={metrics.battery.percent} note={metrics.battery.charging?'заряжается':metrics.battery.secondsLeft!=null?timeLeft(metrics.battery.secondsLeft):undefined}/>}
      {metrics?.netDown!=null&&<div className="gauge gauge--net" title="Скорость сети: приём и отдача">
        <div className="gauge__row"><span>Сеть</span><strong>↓ {speed(metrics.netDown)}</strong></div>
        <div className="gauge__row gauge__row--sub"><span/><em>↑ {speed(metrics.netUp??0)}</em></div>
      </div>}
    </aside>

    {wakeMissing&&<button className="home__wake" onClick={()=>setScreen('voice')}>Обращение «Юки» не записано — записать</button>}
    <QuickRow onError={setError}/>
  </div>
}

/** Показатель: подпись, число и тонкая полоса — как в правом углу Astra. */
function Gauge({label,value,note,title}:{label:string;value:number|null;note?:string;title?:string}){
  return <div className="gauge" title={title}>
    <div className="gauge__row"><span>{label}</span>{note&&<em>{note}</em>}<strong>{value===null?'—':`${Math.round(value)}%`}</strong></div>
    <div className="gauge__track"><i style={{width:`${Math.min(100,Math.max(0,value??0))}%`}}/></div>
  </div>
}

function WeatherBlock(){
  const [weather,setWeather]=useState<Weather|null>(null),[city,setCity]=useState(''),[edit,setEdit]=useState(false),[note,setNote]=useState('')
  const load=useCallback(async(value:string)=>{try{setWeather(await weatherGet(value));setNote('')}catch(e){setNote(String(e))}},[])
  useEffect(()=>{if(!isTauri())return;void settingGet('weather.city').then(value=>{setCity(value||'');void load(value||'')}).catch(()=>undefined)},[load])
  return <div className="home__block">
    <SunIcon/>
    <div className="home__block-body">
      <button className={weather?'home__temp':'home__temp home__temp--empty'} title={weather?`${weather.place} · ощущается ${Math.round(weather.feelsLike)}° · ветер ${Math.round(weather.windSpeed)} км/ч`:'Указать город'} onClick={()=>setEdit(!edit)}>
        {weather?`${Math.round(weather.temperature)>0?'+':''}${Math.round(weather.temperature)}°`:'—'}
      </button>
      <span>{weather?weather.description:note||'загружаю'}</span>
      {edit&&<form className="home__inline" onSubmit={e=>{e.preventDefault();void settingSet('weather.city',city).then(()=>load(city)).then(()=>setEdit(false))}}>
        <input aria-label="Город для погоды" placeholder="Город или пусто — из Windows" value={city} onChange={e=>setCity(e.target.value)}/>
      </form>}
    </div>
  </div>
}

function RatesBlock(){
  const [rates,setRates]=useState<Rates|null>(null),[failed,setFailed]=useState(false)
  useEffect(()=>{if(!isTauri())return;let dead=false;void ratesGet('USD',['EUR']).then(d=>{if(!dead)setRates(d)}).catch(()=>{if(!dead)setFailed(true)});return()=>{dead=true}},[])
  const first=rates?.rates[0]
  return <div className="home__block">
    <CurrencyIcon/>
    <div className="home__block-body">
      <span className="home__block-label">{first?`${rates?.base} / ${first.code}`:'курс'}</span>
      <b className="home__rate">{first?`${first.value.toFixed(4)} €`:failed?'—':'…'}</b>
    </div>
  </div>
}

/**
 * Что играет прямо сейчас.
 *
 * Ничего не проигрывает: читает медиасессию Windows и передаёт нажатия тому
 * плееру, который уже играет. Нет сессии — нет и карточки, а не пустая рамка.
 */
/** Ключ дорожки: смена любого из полей означает, что играет уже другое. */
const trackKey=(t:NowPlaying)=>`${t.title}|${t.artist}|${t.source}|${Math.round(t.duration)}`

const GRACE_MS=5000

function NowPlayingCard(){
  const [track,setTrack]=useState<NowPlaying|null>(null),[note,setNote]=useState('')
  // Время считает свой ход от последней сверки с плеером, а не складывается из
  // «ответ + счётчик». Складывать нельзя: ответ приходит с задержкой, и на
  // каждом опросе показания прыгали назад на доли секунды.
  const anchor=useRef<{pos:number;at:number;key:string}|null>(null)
  const [shown,setShown]=useState(0)

  // При переключении дорожки плеер на секунду-другую пропадает из медиасессий
  // Windows. Карточку в это время держим: иначе она мигает на каждом «вперёд».
  const lastSeen=useRef(0)
  const apply=useCallback((next:NowPlaying)=>{
    if(!next.available&&performance.now()-lastSeen.current<GRACE_MS)return
    if(next.available)lastSeen.current=performance.now()
    setTrack(next)
    const key=trackKey(next)
    const base=anchor.current
    const predicted=base&&base.key===key ? base.pos+(performance.now()-base.at)/1000 : null
    // Переставляем опору только на смене дорожки, на паузе или при настоящем
    // расхождении. Мелкую разницу игнорируем — это задержка ответа, а не
    // перемотка, и дёргать из-за неё показания незачем.
    if(predicted===null||!next.playing||Math.abs(next.position-predicted)>1.5){
      anchor.current={pos:next.position,at:performance.now(),key}
    }
  },[])

  // Раз в две секунды и только у видимого окна: спрятанной в трей карточке
  // опрашивать медиасессию Windows незачем, а это WinRT-вызов, не чтение поля.
  const reload=useCallback(()=>{
    if(!isTauri()||document.hidden||document.documentElement.dataset.paused)return
    void mediaNowPlaying().then(apply).catch(()=>undefined)
  },[apply])
  useEffect(()=>{reload();const timer=setInterval(reload,2000);return()=>clearInterval(timer)},[reload])

  const playing=track?.playing??false
  useEffect(()=>{
    const advance=()=>{
      const base=anchor.current
      if(!base)return
      setShown(playing?base.pos+(performance.now()-base.at)/1000:base.pos)
    }
    advance();const timer=setInterval(advance,500);return()=>clearInterval(timer)
  },[playing,track])

  if(!track?.available)return null
  const position=track.duration>0?Math.min(shown,track.duration):shown
  const progress=track.duration>0?Math.min(100,position/track.duration*100):0

  // Ошибку показываем, а не глотаем: молча не сработавшая кнопка выглядит
  // сломанным приложением, хотя отказал плеер.
  const press=(action:'pause'|'play'|'next'|'previous')=>()=>{
    setNote('')
    // Состояние меняем сразу, не дожидаясь ответа: иначе значок остаётся
    // прежним ещё полсекунды, и следующее нажатие уходит в другую сторону.
    if(action==='pause'||action==='play')setTrack(t=>t?{...t,playing:action==='play'}:t)
    void mediaControl(action)
      .then(()=>[300,900,1800].forEach(delay=>setTimeout(reload,delay)))
      .catch((e:unknown)=>{setNote(String(e));reload()})
  }
  return <article className="player">
    <div className="player__head">
      <span className="player__art" aria-hidden="true"><NoteIcon/></span>
      <div className="player__meta"><strong>{track.title||'Воспроизведение'}</strong><span>{track.artist||track.source.replace(/\.exe$/i,'')||'неизвестный источник'}</span></div>
    </div>
    {track.duration>0&&<div className="player__line">
      <time>{clock(position)}</time>
      <div className="player__track"><i style={{width:`${progress}%`}}/></div>
      <time>{clock(track.duration)}</time>
    </div>}
    <div className="player__controls">
      <button aria-label="Предыдущая" disabled={!track.canPrevious} title={track.canPrevious?'Предыдущая':'Плеер не поддерживает'} onClick={press('previous')}><PrevIcon/></button>
      <button className="player__play" aria-label={track.playing?'Пауза':'Играть'} disabled={!track.canPause} onClick={press(track.playing?'pause':'play')}>{track.playing?<PauseIcon/>:<PlayIcon/>}</button>
      <button aria-label="Следующая" disabled={!track.canNext} title={track.canNext?'Следующая':'Плеер не поддерживает'} onClick={press('next')}><NextIcon/></button>
    </div>
    {note&&<small className="player__note">{note}</small>}
  </article>
}

/** Значки в нижнем углу: напоминания со счётчиком, заметки, активность. */
function QuickRow({onError}:{onError:(text:string)=>void}){
  const setScreen=useUiStore(s=>s.setScreen)
  const [items,setItems]=useState<Reminder[]>([]),[open,setOpen]=useState(false)
  const [text,setText]=useState(''),[date,setDate]=useState('')
  const reload=useCallback(()=>{if(isTauri())void reminderList(false).then(setItems).catch(()=>undefined)},[])
  useEffect(()=>{reload();const timer=setInterval(reload,30000);const off=listen('yuki://reminder-fired',reload);return()=>{clearInterval(timer);void off.then(f=>f())}},[reload])
  async function save(){try{const due=new Date(date).getTime()/1000;if(!Number.isFinite(due))throw new Error('Укажите дату и время');await reminderCreate(text,Math.floor(due));setText('');setDate('');reload()}catch(e){onError(String(e))}}
  return <div className="home__quick">
    {open&&<div className="home__popover" role="dialog" aria-label="Напоминания">
      {items.length===0
        ?<p className="home__popover-empty">Пока ничего не запланировано.</p>
        :items.map(item=><div key={item.id} className="home__popover-row"><span>{item.text}<time>{new Date(item.dueAt*1000).toLocaleString('ru-RU',{day:'numeric',month:'short',hour:'2-digit',minute:'2-digit'})}</time></span><button aria-label={`Отменить: ${item.text}`} onClick={()=>void reminderDelete(item.id).then(reload).catch(()=>undefined)}>×</button></div>)}
      <form onSubmit={e=>{e.preventDefault();void save()}}>
        <input required value={text} onChange={e=>setText(e.target.value)} placeholder="О чём напомнить" aria-label="Текст напоминания"/>
        <input required type="datetime-local" value={date} onChange={e=>setDate(e.target.value)} aria-label="Дата и время"/>
        <button>Создать</button>
      </form>
    </div>}
    <button aria-label={`Напоминания: ${items.length}`} aria-expanded={open} onClick={()=>setOpen(!open)}>
      <CheckIcon/>{items.length>0&&<b>{items.length}</b>}
    </button>
    <button aria-label="Заметки" onClick={()=>setScreen('notes')}><DocIcon/></button>
    <button aria-label="Активность" onClick={()=>setScreen('activity')}><BellIcon/></button>
  </div>
}

// ── Значки ──────────────────────────────────────────────────────────────────────

const line={stroke:'currentColor',strokeWidth:1.6,strokeLinecap:'round' as const,strokeLinejoin:'round' as const,fill:'none'}
function SunIcon(){return <svg className="home__icon" width="32" height="32" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="4.2" {...line}/><path d="M12 2.6v2.2M12 19.2v2.2M2.6 12h2.2M19.2 12h2.2M5.4 5.4l1.6 1.6M17 17l1.6 1.6M18.6 5.4L17 7M7 17l-1.6 1.6" {...line}/></svg>}
function CurrencyIcon(){return <svg className="home__icon" width="32" height="32" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3v18" {...line}/><path d="M16 7.2c-.8-1.3-2.3-2-4-2-2.2 0-3.6 1.1-3.6 2.8 0 4 7.6 2.2 7.6 6.2 0 1.9-1.7 3-4 3-1.9 0-3.4-.8-4.2-2.2" {...line}/></svg>}
function ChatIcon(){return <svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 6.5A2.5 2.5 0 0 1 6.5 4h11A2.5 2.5 0 0 1 20 6.5v7a2.5 2.5 0 0 1-2.5 2.5H9l-5 4z" {...line}/></svg>}
function MicIcon(){return <svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true"><rect x="9" y="3" width="6" height="11" rx="3" {...line}/><path d="M5.5 11.5a6.5 6.5 0 0 0 13 0M12 18v3" {...line}/></svg>}
function NoteIcon(){return <svg width="17" height="17" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 18V5l10-2v13" {...line}/><circle cx="6.5" cy="18" r="2.5" {...line}/><circle cx="16.5" cy="16" r="2.5" {...line}/></svg>}
function PrevIcon(){return <svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true"><path d="M18 5v14L8 12zM6 5v14" {...line}/></svg>}
function NextIcon(){return <svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true"><path d="M6 5v14l10-7zM18 5v14" {...line}/></svg>}
function PlayIcon(){return <svg width="17" height="17" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 4.5v15l13-7.5z" {...line}/></svg>}
function PauseIcon(){return <svg width="17" height="17" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 4.5v15M15 4.5v15" {...line}/></svg>}
function CheckIcon(){return <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true"><rect x="3.5" y="3.5" width="17" height="17" rx="3.5" {...line}/><path d="M8 12.2l2.7 2.8L16 9.5" {...line}/></svg>}
function DocIcon(){return <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true"><path d="M6 3h9l4 4v14a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z" {...line}/><path d="M14 3v5h5M9 13h6M9 17h4" {...line}/></svg>}
function BellIcon(){return <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true"><path d="M6 9a6 6 0 1 1 12 0c0 4 1.4 5.5 1.4 5.5H4.6S6 13 6 9z" {...line}/><path d="M10 18.5a2 2 0 0 0 4 0" {...line}/></svg>}
