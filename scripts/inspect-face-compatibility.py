"""Check exact named morph compatibility. Never guess facial morphs across models."""
import pathlib, json, struct, yaml, re
models={}
for path in (pathlib.Path.home()/'AppData/Roaming/ai.yuki.companion/companions').glob('*.vrm'):
 data=path.read_bytes();n=struct.unpack_from('<I',data,12)[0];g=json.loads(data[20:20+n]);del data
 names={name for mesh in g.get('meshes',[]) for name in mesh.get('extras',{}).get('targetNames',[])}
 models[path.stem]=names
report=[]
for path in pathlib.Path('tools/motion-converter/Assets/Motions').rglob('*.anim'):
 if path.parent.name not in ['shinano_mod_face (1)','リア-アリス用表情_13種']: continue
 text=path.read_text(encoding='utf8')
 if 'blendShape.' not in text: continue
 text=re.sub(r'^%.*\n','',text,flags=re.M);text=re.sub(r'^--- !u!\d+ &\d+','---',text,flags=re.M)
 clip=yaml.safe_load(text)['AnimationClip']
 curves=clip.get('m_FloatCurves',[])
 active={c['attribute'][11:] for c in curves if str(c.get('attribute','')).startswith('blendShape.') and any(abs(k.get('value',0))>.01 for k in c['curve'].get('m_Curve',[]))}
 if not active:continue
 report.append({'clip':path.as_posix(),'activeMorphs':sorted(active),'compatibility':{model: {'matched':len(active&names),'required':len(active),'complete':active<=names} for model,names in models.items()}})
pathlib.Path('tmp/face-compatibility.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf8')
print(json.dumps({'facialClips':len(report),'fullyCompatible':{model:sum(r['compatibility'][model]['complete'] for r in report) for model in models}},ensure_ascii=False))
