import { VRMLoaderPlugin, VRMUtils, type VRM } from '@pixiv/three-vrm'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { avatarModelBytes, settingGet } from '../bridge'

// A WebView owns its models. Never share a mutable skeleton between renderers.
// Keep one released character warm for a minute, then release its GPU resources.
const cache = new Map<string, { vrm: VRM; busy: boolean; yaw: number; released: (()=>void)[] }>()
const pending = new Map<string, Promise<VRM>>()

export async function acquireModel(path: string) {
  const economy = await settingGet('avatar.quality') === 'economy'
  const key = `${path}\0${economy ? 'economy' : 'original'}`
  let entry = cache.get(key)
  const cached = !!entry
  if (!entry) {
    let request = pending.get(key)
    if (!request) {
      request = (async () => {
        const loader = new GLTFLoader()
        loader.register(parser => new VRMLoaderPlugin(parser))
        const gltf = await loader.parseAsync(await avatarModelBytes(path), '')
        const vrm = gltf.userData.vrm as VRM | undefined
        if (!vrm) throw new Error('Файл не содержит VRM-персонажа')
        VRMUtils.removeUnnecessaryVertices(vrm.scene)
        VRMUtils.combineSkeletons(vrm.scene)
        VRMUtils.combineMorphs(vrm)
        VRMUtils.rotateVRM0(vrm)
        vrm.scene.traverse(object => { object.frustumCulled = false })
        return vrm
      })()
      pending.set(key, request)
    }
    try {
      const vrm = await request
      entry = cache.get(key) || { vrm, busy: false, yaw: vrm.scene.rotation.y, released: [] }
      cache.set(key, entry)
    } finally { pending.delete(key) }
  }
  if (entry.busy) await new Promise<void>(resolve => entry.released.push(resolve))
  cache.delete(key); cache.set(key, entry)
  entry.busy = true
  const { vrm } = entry
  vrm.humanoid.resetNormalizedPose()
  vrm.expressionManager?.resetValues()
  vrm.scene.position.set(0, 0, 0)
  vrm.scene.rotation.set(0, entry.yaw, 0)
  vrm.update(0)
  vrm.springBoneManager?.reset()
  return { vrm, cached, economy, release() {
    entry.busy = false
    vrm.scene.removeFromParent()
    entry.released.shift()?.()
    const inactive = [...cache.entries()].filter(([,value]) => !value.busy)
    while (inactive.length > 1) {
      const [oldKey, old] = inactive.shift()!
      cache.delete(oldKey); VRMUtils.deepDispose(old.vrm.scene)
    }
    setTimeout(()=>{
      if(!entry.busy && cache.get(key)===entry) {cache.delete(key);VRMUtils.deepDispose(entry.vrm.scene)}
    },60000)
  } }
}
