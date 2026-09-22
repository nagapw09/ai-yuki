import {describe,it,expect} from 'vitest'
import {assignedMotion,isPoseClip} from './motions'
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
