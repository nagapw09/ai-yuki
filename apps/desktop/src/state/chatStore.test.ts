import { beforeEach, describe, expect, it, vi } from 'vitest'
vi.hoisted(()=>{const data=new Map<string,string>();const storage={getItem:(key:string)=>data.get(key)??null,setItem:(key:string,value:string)=>data.set(key,value),removeItem:(key:string)=>data.delete(key)};Object.defineProperty(globalThis,'localStorage',{configurable:true,value:storage});Object.defineProperty(globalThis,'window',{configurable:true,value:{localStorage:storage}})})
import { dropDangling, useChatStore } from './chatStore'
describe('failed conversation turns',()=>{
  beforeEach(()=>useChatStore.getState().clear())
  it('keeps the partial answer and full failure when the next message starts',()=>{
    const s=useChatStore.getState();s.startTurn('Открой Telegram');s.appendDelta('Ищу группу…');s.failTurn('HTTP 404: model not found');s.startTurn('Запиши заметку')
    expect(useChatStore.getState().entries[1]).toMatchObject({role:'assistant',text:'Ищу группу…',error:'HTTP 404: model not found'})
    expect(JSON.parse(localStorage.getItem('yuki-astra-conversation')!).state.entries[1].error).toBe('HTTP 404: model not found')
  })
  it('keeps failed tools and closes interrupted tools',()=>{
    const s=useChatStore.getState();s.startTurn('Найди чат');s.upsertTool({id:'uia',toolId:'uia',label:'Чтение',state:'error',detail:'COM failure'});s.upsertTool({id:'image',toolId:'image',label:'Снимок',state:'running'});s.failTurn('Provider failure');s.failTurn('Provider failure')
    expect(useChatStore.getState().entries).toHaveLength(2)
    expect(useChatStore.getState().entries[1]?.tools.map(t=>t.state)).toEqual(['error','error'])
  })
})
describe('dropDangling', () => {
  it('removes tool calls that never got a result', () => {
    const history = [
      { role: 'user' as const, content: [{ type: 'text' as const, text: 'залог?' }] },
      { role: 'assistant' as const, content: [{ type: 'text' as const, text: 'Сейчас поищу.' }, { type: 'tool_use' as const, id: 'a', name: 'search_document', input: {} }] },
      { role: 'user' as const, content: [{ type: 'text' as const, text: 'другое' }] },
      { role: 'assistant' as const, content: [{ type: 'tool_use' as const, id: 'b', name: 'set_mute', input: {} }] },
      { role: 'user' as const, content: [{ type: 'tool_result' as const, toolUseId: 'b', content: 'ok', isError: false }] },
    ]
    const clean = dropDangling(history)
    expect(clean[1]!.content).toEqual([{ type: 'text', text: 'Сейчас поищу.' }])
    expect(clean[3]!.content[0]).toMatchObject({ id: 'b' })
  })
})
