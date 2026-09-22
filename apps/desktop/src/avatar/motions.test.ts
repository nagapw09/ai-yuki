import {describe,it,expect} from 'vitest'
import {assignedMotion,isPoseClip,pickMotion} from './motions'
describe('authored motions',()=>{
  it('never substitutes a fake dance or seated pose when assets are absent',()=>{
    expect(assignedMotion('dance',['spin','greeting'],{})).toBeUndefined()
    expect(assignedMotion('sit',['squat'],{})).toBeUndefined()
    expect(assignedMotion('wave',['greeting'],{})).toBe('greeting')
  })
  it('uses an explicit assignment and permits unassigning an action',()=>{
    expect(assignedMotion('dance',['my-dance'],{dance:'my-dance'})).toBe('my-dance')
    expect(assignedMotion('wave',['greeting'],{wave:''})).toBeUndefined()
  })
})

describe('pose or motion',()=>{
  const clip=(frames:number,duration:number)=>({duration,tracks:[{times:{length:frames}}]})
  it('holds a pose and plays a short motion',()=>{
    // Измерено на настоящих файлах: поза — 2 кадра даже при длительности в
    // секунду, цикл бега на 0,7 с — 23 кадра, танец на 0,65 с — 21.
    expect(isPoseClip(clip(2,1.0))).toBe(true)
    expect(isPoseClip(clip(2,0.02))).toBe(true)
    expect(isPoseClip(clip(3,0.04))).toBe(true)
    expect(isPoseClip(clip(23,0.70))).toBe(false)
    expect(isPoseClip(clip(21,0.65))).toBe(false)
    expect(isPoseClip(clip(432,14.33))).toBe(false)
  })
  it('falls back to duration when a clip reports no keyframes',()=>{
    expect(isPoseClip({duration:0.04,tracks:[]})).toBe(true)
    expect(isPoseClip({duration:9,tracks:[]})).toBe(false)
  })
})

describe('variety',()=>{
  const clips=['greeting','peace','NiziPoseSet__NZ_Pose01','_02_FREE_POSESET_Vol1_ver100__5_sit1a','TisyeFreePose__TisyeFree02']
  it('never repeats the same clip twice in a row when there is a choice',()=>{
    for(let i=0;i<20;i++) expect(pickMotion('wave',clips,{},'greeting')).toBe('peace')
  })
  it('finds poses by set name and keeps seated ones out of standing poses',()=>{
    const seen=new Set(Array.from({length:40},(_,i)=>pickMotion('pose',clips,{},'',()=>i/40)))
    expect(seen).toEqual(new Set(['NiziPoseSet__NZ_Pose01','TisyeFreePose__TisyeFree02']))
  })
  it('an explicit assignment wins over variety',()=>{
    expect(pickMotion('wave',clips,{wave:'peace'},'peace')).toBe('peace')
  })
})
