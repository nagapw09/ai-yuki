using System;
using System.IO;
using System.Linq;
using System.Collections.Generic;
using UnityEngine;
using UnityEditor;
using UnityEngine.Animations;
using UnityEngine.Playables;

// Offline conversion of the user's authored clips, evaluated by Unity's humanoid retargeter.
public static class BakeMotions {
    [Serializable] public class Bone { public string name; public int parent; public Vector3 position; public Quaternion rotation; }
    [Serializable] public class Frame { public float time; public Vector3 hips; public Quaternion[] rotations; }
    [Serializable] public class Motion { public string name, source; public float duration; public Bone[] bones; public Frame[] frames; }
    static readonly string Root = Path.GetFullPath("Output");
    public static void Run() {
        Directory.CreateDirectory(Root);
        var modelPath = "Assets/Motions/Arisa/Arisa.fbx";
        var importer = (ModelImporter)AssetImporter.GetAtPath(modelPath);
        importer.animationType = ModelImporterAnimationType.Human;
        importer.avatarSetup = ModelImporterAvatarSetup.CreateFromThisModel;
        importer.SaveAndReimport();
        var prefab = AssetDatabase.LoadAssetAtPath<GameObject>(modelPath);
        var all = AssetDatabase.FindAssets("t:AnimationClip", new[]{"Assets/Motions"})
            .Select(AssetDatabase.GUIDToAssetPath).Where(p=>p.EndsWith(".anim")).ToArray();
        var report = new List<string>();
        foreach(var path in all) {
            var go = UnityEngine.Object.Instantiate(prefab);
            PlayableGraph graph = default;
            try {
                var animator = go.GetComponent<Animator>() ?? go.AddComponent<Animator>();
                if (animator.avatar == null || !animator.avatar.isHuman || !animator.avatar.isValid) throw new Exception("Invalid humanoid avatar");
                animator.applyRootMotion = false;
                animator.cullingMode = AnimatorCullingMode.AlwaysAnimate;
                var clip = AssetDatabase.LoadAssetAtPath<AnimationClip>(path);
                var bindings=AnimationUtility.GetCurveBindings(clip);
                if(!bindings.Any(b=>b.type==typeof(Animator)||b.type==typeof(Transform))) {
                    report.Add("FACIAL_ONLY (requires matching model blendshapes): "+path);continue;
                }
                var bones = new List<Bone>(); var transforms = new List<Transform>();
                foreach(HumanBodyBones b in Enum.GetValues(typeof(HumanBodyBones))) {
                    if(b==HumanBodyBones.LastBone) continue;
                    var t=animator.GetBoneTransform(b); if(t==null) continue;
                    transforms.Add(t);
                    bones.Add(new Bone { name=char.ToLowerInvariant(b.ToString()[0])+b.ToString().Substring(1) });
                }
                var parents = new Transform[transforms.Count];
                for(int i=0;i<transforms.Count;i++) {
                    var p=transforms[i].parent;
                    while(p!=null && !transforms.Contains(p)) p=p.parent;
                    parents[i]=p ?? go.transform;
                    bones[i].parent=p==null ? -1 : transforms.IndexOf(p);
                    bones[i].position=parents[i].InverseTransformPoint(transforms[i].position);
                    bones[i].rotation=Quaternion.Inverse(parents[i].rotation)*transforms[i].rotation;
                }
                graph=PlayableGraph.Create("YukiMotionBake");
                graph.SetTimeUpdateMode(DirectorUpdateMode.Manual);
                var playable=AnimationClipPlayable.Create(graph,clip);
                playable.SetApplyFootIK(false); playable.SetApplyPlayableIK(false);
                AnimationPlayableOutput.Create(graph,"Motion",animator).SetSourcePlayable(playable);
                graph.Play();
                var frames=new List<Frame>();
                int count=Math.Max(2,Mathf.CeilToInt(clip.length*30)+1);
                int hips= bones.FindIndex(b=>b.name=="hips");
                for(int n=0;n<count;n++) {
                    float t=Math.Min(n/30f,clip.length);
                    playable.SetTime(t); graph.Evaluate(0);
                    frames.Add(new Frame { time=t, hips=parents[hips].InverseTransformPoint(transforms[hips].position),
                        rotations=transforms.Select((bone,i)=>Quaternion.Inverse(parents[i].rotation)*bone.rotation).ToArray() });
                }
                var motion=new Motion { name=clip.name,source=path,duration=clip.length,bones=bones.ToArray(),frames=frames.ToArray() };
                var filename=Path.GetFileName(Path.GetDirectoryName(path))+"__"+Path.GetFileNameWithoutExtension(path)+".json";
                File.WriteAllText(Path.Combine(Root,filename),JsonUtility.ToJson(motion));
                report.Add("OK "+path+" "+clip.length+" seconds, "+frames.Count+" frames");
            } catch(Exception e) {report.Add("FAILED "+path+" "+e); Debug.LogError(e);}
            finally {if(graph.IsValid()) graph.Destroy(); UnityEngine.Object.DestroyImmediate(go);}
        }
        File.WriteAllLines(Path.Combine(Root,"report.txt"),report);
        Debug.Log("YUKI_BAKE_FINISHED "+report.Count);
    }
}
