export const ACTIONS = [['fidget','Размяться'],['wave','Приветствие'],['sit','Сидение'],['stand','Покой'],['walk','Прогулка'],['dance','Танец'],['stretch','Потягивание'],['lie','Лежать'],['sleep','Сон']] as const
export type Action = typeof ACTIONS[number][0]
export type MotionMap = Partial<Record<Action,string>>
const defaults:Partial<Record<Action,string[]>>={wave:['greeting'],stand:['Arisa__NewIdle02','Arisa__NewIdle'],fidget:['showcase'],sit:['chair_sit__chair_sit'],walk:['Arisa__NewWalk'],dance:['Meme_Animation__Unwelcome School_dance','VRSuya_INTERNET_YAMERO_Released_260709__VRSuya_INTERNET_YAMERO']}
export function assignedMotion(action:Action,clips:readonly string[],map:MotionMap):string|undefined {
  if(Object.prototype.hasOwnProperty.call(map,action)) return clips.includes(map[action]||'')?map[action]:undefined
  return defaults[action]?.find(name=>clips.includes(name))
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
