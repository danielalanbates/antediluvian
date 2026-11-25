import bpy, sys, os, glob
# Re-export every .blend under src as GLB with ALL actions (FBX route drops takes).
src, dst = sys.argv[-2], sys.argv[-1]
for f in sorted(glob.glob(src + '/**/*.blend', recursive=True)):
    rel = os.path.relpath(f, src)
    out = os.path.join(dst, os.path.splitext(rel)[0].replace(' ', '_') + '.glb')
    os.makedirs(os.path.dirname(out), exist_ok=True)
    try:
        bpy.ops.wm.open_mainfile(filepath=f)
        for o in list(bpy.data.objects):
            if o.type in ('LIGHT', 'CAMERA'):
                bpy.data.objects.remove(o)
        n = len(bpy.data.actions)
        bpy.ops.export_scene.gltf(filepath=out, export_format='GLB',
                                  export_animations=True, export_animation_mode='ACTIONS')
        print('OK', n, out)
    except Exception as e:
        print('FAIL', f, str(e)[:120])
