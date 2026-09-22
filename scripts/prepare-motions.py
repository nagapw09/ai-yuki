"""Extract only motion data into an isolated Unity project; never import package scripts."""
import io, json, pathlib, tarfile, zipfile
source = pathlib.Path('C:/Users/alex/Downloads')
project = pathlib.Path('tools/motion-converter')
assets = project / 'Assets' / 'Motions'
assets.mkdir(parents=True, exist_ok=True)
(project / 'Assets' / 'Editor').mkdir(exist_ok=True)
(project / 'Packages').mkdir(exist_ok=True)
(project / 'Packages' / 'manifest.json').write_text('{"dependencies":{"com.unity.modules.animation":"1.0.0","com.unity.modules.jsonserialize":"1.0.0"}}')
names = ['Meme_Animation.zip', 'chair_sit.zip', 'bn0010_hands_heart.zip', 'NiziPoseSet.zip', 'NiziPoseSet_Re.zip', 'TisyeFreePose.zip', 'TisyePosePack2_Free.zip', 'VRSuya_INTERNET_YAMERO_Released_260709.zip', 'VRSuya_Loli_Kami_Requiem_Released_260709.zip', 'Arisa.zip']
names += ['_02_FREE_POSESET_Vol.1_ver.1.00.zip','Deviate_Pose_+_SceneSet.zip','shinano_mod_face (1).zip','リア-アリス用表情_13種.zip','_AFK_うまぴょい.zip','_muamm_pose.zip']
inventory = []
def save(pack, name, data, meta=None):
    if pathlib.Path(name).suffix.lower() not in ['.anim', '.fbx']: return
    folder = assets / pack
    folder.mkdir(exist_ok=True)
    dest = folder / pathlib.PurePosixPath(name).name
    if dest.exists() and dest.read_bytes() != data: dest = folder / (str(len(inventory)) + '_' + dest.name)
    dest.write_bytes(data)
    if meta: pathlib.Path(str(dest) + '.meta').write_bytes(meta)
    inventory.append({'pack': pack, 'source': name, 'asset': dest.as_posix(), 'bytes': len(data)})
for archive in names:
    path = source / archive
    if not path.exists(): continue
    with zipfile.ZipFile(path) as z:
        for name in z.namelist():
            if name.endswith(('.anim', '.fbx', '.FBX')):
                save(path.stem, name, z.read(name), z.read(name+'.meta') if name+'.meta' in z.namelist() else None)
            elif name.endswith('.unitypackage'):
                with tarfile.open(fileobj=io.BytesIO(z.read(name)), mode='r:gz') as t:
                    members = {m.name: m for m in t.getmembers() if m.isfile()}
                    for entry, member in members.items():
                        if not entry.endswith('/pathname'): continue
                        resource = t.extractfile(member).read().decode('utf8', 'replace').strip()
                        if not resource.endswith(('.anim', '.fbx', '.FBX')): continue
                        prefix = entry.rsplit('/', 1)[0]
                        meta = members.get(prefix+'/asset.meta')
                        save(path.stem, resource, t.extractfile(members[prefix+'/asset']).read(), t.extractfile(meta).read() if meta else None)
(project/'inventory.json').write_text(json.dumps(inventory, ensure_ascii=False, indent=2), encoding='utf8')
print(json.dumps({'files':len(inventory), 'packs':{n:sum(i['pack']==pathlib.Path(n).stem for i in inventory) for n in names}},ensure_ascii=False))
