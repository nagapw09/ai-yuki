export const ACTIONS = [['fidget','Размяться'],['wave','Приветствие'],['pose','Позировать'],['sit','Сидение'],['stand','Покой'],['walk','Прогулка'],['dance','Танец'],['stretch','Потягивание'],['lie','Лежать'],['sleep','Сон']] as const
export type Action = typeof ACTIONS[number][0]
export type MotionMap = Partial<Record<Action,string>>

/**
 * Наборы клипов по умолчанию.
 *
 * У каждого действия несколько вариантов, а не один: персонаж, который
 * двадцатый раз подряд машет одним и тем же жестом, выглядит заводной куклой.
 * Порядок важен только для первого варианта — он же проверяется на наличие.
 */
const defaults:Partial<Record<Action,string[]>>={
  wave:['greeting','peace','bn0010_hands_heart__bn0010_hands_heart'],
  stand:['Arisa__NewIdle02','Arisa__NewIdle'],
  fidget:['showcase','spin','squat','peace','shoot','Meme_Animation__kyofu_allback_step','Meme_Animation__INTERNET YAMERO_up_down'],
  sit:['chair_sit__chair_sit'],
  walk:['Arisa__NewWalk'],
  // Подобраны по снимкам: лёжа на боку и на спине.
  lie:['TisyeFreePose__TisyeFree09','_muamm_pose__mpose5'],
  // Сидя на полу, обняв колени, и на пятках — глаза во сне закрываются сами.
  sleep:['_muamm_pose__mpose4','_muamm_pose__mpose6'],
  dance:['Meme_Animation__Unwelcome School_dance','VRSuya_INTERNET_YAMERO_Released_260709__VRSuya_INTERNET_YAMERO','VRSuya_Loli_Kami_Requiem_Released_260709__VRSuya_Loli_Kami_Requiem','_AFK_うまぴょい__うまぴょい'],
}

/**
 * Позы узнаются по имени набора: их в библиотеке десятки, и перечислять каждую
 * вручную значит забыть новые. Сидячие позы сюда не входят — стоя «сидеть»
 * в воздухе хуже, чем не позировать вовсе.
 */
const POSE_NAME=/(pose|stand\d|TisyeFree|kasa)/i
const SEATED=/sit/i
/**
 * Позы, которые не годятся для «встать в позу»: вверх ногами, на четвереньках,
 * а ещё те, что уже отданы «лечь» и «поспать». Найдены по снимкам библиотеки —
 * случайная стойка на голове посреди рабочего дня выглядит поломкой.
 */
const NOT_STANDING=new Set(['TisyeFreePose__TisyeFree10','_muamm_pose__mpose7','_muamm_pose__mpose8','_muamm_pose__mpose1'])

function pool(action:Action,clips:readonly string[]):string[] {
  if(action==='pose') {
    const taken=new Set([...(defaults.lie??[]),...(defaults.sleep??[])])
    return clips.filter(name=>POSE_NAME.test(name)&&!SEATED.test(name)&&!NOT_STANDING.has(name)&&!taken.has(name))
  }
  return (defaults[action]??[]).filter(name=>clips.includes(name))
}

/** Какой клип отвечает за действие — для проверки, доступно ли оно вообще. */
export function assignedMotion(action:Action,clips:readonly string[],map:MotionMap):string|undefined {
  if(Object.prototype.hasOwnProperty.call(map,action)) return clips.includes(map[action]||'')?map[action]:undefined
  return pool(action,clips)[0]
}

/**
 * Какой клип сыграть сейчас: случайный из набора, но не тот, что был только что.
 *
 * Назначенный человеком клип — единственный: раз выбрал сам, подменять его
 * разнообразием значит спорить с ним.
 */
export function pickMotion(action:Action,clips:readonly string[],map:MotionMap,last='',random:()=>number=Math.random):string|undefined {
  if(Object.prototype.hasOwnProperty.call(map,action)) return assignedMotion(action,clips,map)
  const options=pool(action,clips)
  const fresh=options.length>1?options.filter(name=>name!==last):options
  return fresh[Math.floor(random()*fresh.length)]
}
export function parseMotionMap(value:string|null):MotionMap {try{return JSON.parse(value||'{}') as MotionMap}catch{return {}}}
export function motionLabel(name:string){return name.replace(/^.*?__/,'').replaceAll('_',' ')}

/**
 * Поза это или движение.
 *
 * Поза записана одним-двумя ключевыми кадрами: проигранная «один раз» она
 * мелькает за сотые доли секунды, и понять, что это было, невозможно. Настоящий
 * клип даже самый короткий содержит десятки кадров — цикл бега на 0,7 секунды
 * это 23 кадра, поза на целую секунду это 2.
 *
 * Считаем по кадрам, а не по длительности: у позы она бывает и 1,00 секунды,
 * и тогда от бега её не отличить.
 */
export const POSE_MAX_KEYFRAMES = 4
export function isPoseClip(clip:{duration:number;tracks:readonly {times:{length:number}}[]}):boolean {
  const frames = clip.tracks.reduce((most,track)=>Math.max(most,track.times.length),0)
  return frames > 0 ? frames <= POSE_MAX_KEYFRAMES : clip.duration < 0.2
}
