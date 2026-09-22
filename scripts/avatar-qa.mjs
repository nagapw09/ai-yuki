// Local WebView QA fallback: agent-browser loses its session when Tauri recreates a window.
import { readFile, writeFile } from 'node:fs/promises'
const [label = 'avatar', operation = 'state', file] = process.argv.slice(2)
const targets = await (await fetch('http://127.0.0.1:9223/json/list')).json()
async function connect(url) {
  const socket = new WebSocket(url)
  await new Promise((resolve,reject)=>{socket.onopen=resolve;socket.onerror=reject})
  let serial=0
  const waiting=new Map()
  socket.onmessage=event=>{const data=JSON.parse(event.data); const task=waiting.get(data.id);if(task){waiting.delete(data.id);data.error?task.reject(data.error):task.resolve(data.result)}}
  return {socket,call(method,params={}) {return new Promise((resolve,reject)=>{const id=++serial;waiting.set(id,{resolve,reject});socket.send(JSON.stringify({id,method,params}))})}}
}
let client
for (const target of targets) {
  const c=await connect(target.webSocketDebuggerUrl)
  const result=await c.call('Runtime.evaluate',{expression:'window.__TAURI_INTERNALS__?.metadata.currentWindow.label',returnByValue:true})
  if(result.result.value===label){client=c;break}
  c.socket.close()
}
if(!client)throw new Error(`No ${label} WebView`)
try {
  if(operation==='screenshot'){
    const result=await client.call('Page.captureScreenshot',{format:'png'})
    await writeFile(file,Buffer.from(result.data,'base64')); console.log(file)
  } else {
    const expression=operation.startsWith('eval')?await readFile(file,'utf8'):'document.querySelector("canvas")?.yukiDebug?.()'
    const result=await client.call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true,userGesture:true})
    if(result.exceptionDetails)throw new Error(JSON.stringify(result.exceptionDetails))
    console.log(JSON.stringify(result.result.value,null,2))
    if(operation==='eval-screenshot'){
      const shot=await client.call('Page.captureScreenshot',{format:'png'})
      await writeFile(process.argv[5],Buffer.from(shot.data,'base64'))
    }
  }
}finally{client.socket.close()}
