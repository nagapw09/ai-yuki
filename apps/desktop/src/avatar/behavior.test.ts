import { describe, expect, it } from 'vitest'
import { CompanionBehavior, type BehaviorInput } from './behavior'

const idle: BehaviorInput = {state:'idle',mode:'playful',musicEnabled:true,musicPlaying:false,mediaAvailable:true,idleSeconds:0,canWalk:true}
describe('companion autonomy', () => {
  it('does not choose missing dances or poses, even during music and absence', () => {
    const brain=new CompanionBehavior(()=>0)
    const limited={...idle,availableActions:['builtin:stand','builtin:wave']}
    expect(brain.tick(1,{...limited,musicPlaying:true})).toBe('builtin:wave')
    expect(brain.tick(50,{...limited,idleSeconds:601})).toBe('builtin:stand')
  })
  it('an expression does not freeze autonomous movement for two minutes', () => {
    // Важно само возобновление, а не конкретное занятие: его выбирают
    // внутренние величины, и привязываться к названию здесь незачем.
    const brain=new CompanionBehavior(()=>0)
    brain.manual('emotion:happy',1)
    expect(brain.tick(2,idle)).not.toBeNull()
  })
  it('respects a manual pose, then resumes on request', () => {
    const brain=new CompanionBehavior(()=>0)
    brain.manual('builtin:sit',10)
    expect(brain.tick(40,{...idle,musicPlaying:true})).toBeNull()
    brain.resume(41)
    expect(brain.tick(41,{...idle,musicPlaying:true})).toBe('builtin:dance')
  })
  it('stops wandering to listen and does not restart on each tick', () => {
    const brain=new CompanionBehavior(()=>0)
    brain.manual('builtin:walk',0)
    expect(brain.tick(1,{...idle,state:'listening'})).toBe('builtin:stand')
    expect(brain.tick(2,{...idle,state:'listening'})).toBeNull()
  })
  it('stops dancing when playback stops', () => {
    const brain=new CompanionBehavior(()=>0)
    expect(brain.tick(1,{...idle,musicPlaying:true})).toBe('builtin:dance')
    expect(brain.tick(2,{...idle,musicPlaying:true})).toBeNull()
    expect(brain.tick(3,idle)).toBe('builtin:stand')
  })
  it('sleeps after absence and wakes when the user returns', () => {
    const brain=new CompanionBehavior(()=>0)
    expect(brain.tick(10,{...idle,idleSeconds:601})).toBe('builtin:sleep')
    expect(brain.tick(11,{...idle,idleSeconds:602})).toBeNull()
    expect(brain.tick(12,idle)).toBe('builtin:stretch')
  })
  it('quiet mode and free placement never start autonomous walking', () => {
    const brain=new CompanionBehavior(()=>0.65)
    expect(brain.tick(10,{...idle,mode:'quiet',musicPlaying:true})).toBeNull()
    for(let now=20;now<500;now+=40)expect(brain.tick(now,{...idle,canWalk:false})).not.toBe('builtin:walk')
  })
})

describe('ожидание слова пробуждения',()=>{
  const base = {musicPlaying:false,idleSeconds:5,mediaAvailable:true,mode:'calm' as const,musicEnabled:false,canWalk:true}
  it('не считает дежурный микрофон разговором',()=>{
    // При включённом ожидании имени состояние `listening` держится постоянно.
    // Если принять его за разговор, компаньон замрёт навсегда.
    const brain = new CompanionBehavior(() => 0)
    const first = brain.tick(100,{...base,state:'listening',passive:true})
    expect(first).not.toBeNull()
  })
  it('во время настоящего разговора занимается только собеседником',()=>{
    const brain = new CompanionBehavior(() => 0)
    expect(brain.tick(100,{...base,state:'listening'})).toBeNull()
    expect(brain.tick(200,{...base,state:'thinking',passive:true})).toBeNull()
  })
})

describe('внутренние величины', () => {
  it('тянется к курсору тем сильнее, чем он ближе', () => {
    // Одни и те же условия, разница только в курсоре: рядом с ним у
    // любопытного персонажа приветствие обгоняет прочие занятия.
    const brain = new CompanionBehavior(() => 0)
    brain.restore({energy:0.9,boredom:0.1,affection:0.5,curiosity:1})
    const near = brain.tick(1,{...idle,cursor:{x:0.5,y:0.55}})
    expect(near).toBe('builtin:wave')

    const other = new CompanionBehavior(() => 0)
    other.restore({energy:0.9,boredom:0.1,affection:0.5,curiosity:1})
    expect(other.tick(1,{...idle,cursor:{x:0.02,y:0.02}})).not.toBe('builtin:wave')
  })
  it('уставший ложится спать, отдохнувший — нет', () => {
    const tired = new CompanionBehavior(() => 0)
    tired.restore({energy:0,boredom:0,affection:0.3,curiosity:0.2})
    expect(tired.tick(1,idle)).toBe('builtin:sleep')

    const fresh = new CompanionBehavior(() => 0)
    fresh.restore({energy:1,boredom:0,affection:0.3,curiosity:0.2})
    expect(fresh.tick(1,idle)).not.toBe('builtin:sleep')
  })
  it('внимание человека уменьшает скуку и копит привязанность', () => {
    const brain = new CompanionBehavior(() => 0)
    brain.restore({energy:0.8,boredom:0.9,affection:0.2,curiosity:0.5})
    brain.touched(0.1)
    expect(brain.vitals.affection).toBeGreaterThan(0.2)
    expect(brain.vitals.boredom).toBeLessThan(0.9)
  })
  it('не повторяет то же занятие подряд', () => {
    const brain = new CompanionBehavior(() => 0)
    const first = brain.tick(1,idle)
    const second = brain.tick(400,idle)
    expect(second).not.toBe(first)
  })
})
