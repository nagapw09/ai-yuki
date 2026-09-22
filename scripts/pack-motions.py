"""Package Unity-evaluated humanoid transforms as VRMA; no motion synthesis."""
import json, pathlib, struct, zipfile
root=pathlib.Path('tools/motion-converter/Output')
output=pathlib.Path('tmp/converted-motions')
output.mkdir(exist_ok=True)
def vec(v): return [-v['x'],v['y'],v['z']]
def quat(v): return [v['x'],-v['y'],-v['z'],v['w']]
def bone_name(name):
    for side in ['left','right']:
        if name==side+'ThumbProximal': return side+'ThumbMetacarpal'
        if name==side+'ThumbIntermediate': return side+'ThumbProximal'
    return name
count=0
for file in root.glob('*.json'):
    motion=json.loads(file.read_text(encoding='utf8'))
    frames=motion['frames']
    # Static pose clips may contain two samples at t=0. glTF requires increasing time.
    if len(frames)<2 or frames[-1]['time']<=0:
        frames=[dict(frames[0],time=0),dict(frames[0],time=1)]
    binary=bytearray(); views=[]; accessors=[]; channels=[]; samplers=[]
    def accessor(values,kind):
        flat=[v for row in values for v in row] if isinstance(values[0],list) else values
        start=len(binary);binary.extend(struct.pack('<'+'f'*len(flat),*flat))
        views.append({'buffer':0,'byteOffset':start,'byteLength':len(binary)-start})
        result={'bufferView':len(views)-1,'componentType':5126,'count':len(values),'type':kind}
        if kind=='SCALAR': result.update(min=[min(values)],max=[max(values)])
        accessors.append(result);return len(accessors)-1
    times=accessor([f['time'] for f in frames],'SCALAR')
    def channel(node,path,values,kind):
        samplers.append({'input':times,'output':accessor(values,kind),'interpolation':'LINEAR'})
        channels.append({'sampler':len(samplers)-1,'target':{'node':node,'path':path}})
    nodes=[]; human={}; roots=[]
    for i,bone in enumerate(motion['bones']):
        name=bone_name(bone['name']);human[name]={'node':i}
        nodes.append({'name':name,'translation':vec(bone['position']),'rotation':quat(bone['rotation'])})
    for i,bone in enumerate(motion['bones']):
        if bone['parent']<0: roots.append(i)
        else: nodes[bone['parent']].setdefault('children',[]).append(i)
        channel(i,'rotation',[quat(f['rotations'][i]) for f in frames],'VEC4')
        if bone['name']=='hips': channel(i,'translation',[vec(f['hips']) for f in frames],'VEC3')
    gltf={'asset':{'version':'2.0','generator':'Yuki authored motion converter (Unity humanoid evaluation)'},'scene':0,'scenes':[{'nodes':roots}],'nodes':nodes,'buffers':[{'byteLength':len(binary)}],'bufferViews':views,'accessors':accessors,'animations':[{'name':motion['name'],'channels':channels,'samplers':samplers}],'extensionsUsed':['VRMC_vrm_animation'],'extensions':{'VRMC_vrm_animation':{'specVersion':'1.0','humanoid':{'humanBones':human}}},'extras':{'source':motion['source'],'personalImport':True}}
    data=json.dumps(gltf,separators=(',',':'),ensure_ascii=False).encode('utf8');data+=b' '*((-len(data))%4)
    binary+=b'\0'*((-len(binary))%4)
    glb=struct.pack('<4sII',b'glTF',2,12+8+len(data)+8+len(binary))+struct.pack('<I4s',len(data),b'JSON')+data+struct.pack('<I4s',len(binary),b'BIN\0')+binary
    (output/(file.stem+'.vrma')).write_bytes(glb);count+=1
if not count: raise SystemExit('No Unity exports found. Activate Unity Personal and run BakeMotions.Run first.')
with zipfile.ZipFile('tmp/Yuki-Personal-Motions.zip','w',zipfile.ZIP_DEFLATED) as archive:
    for file in output.glob('*.vrma'): archive.write(file,file.name)
print(f'{count} authored motions packed into tmp/Yuki-Personal-Motions.zip (personal use only)')
