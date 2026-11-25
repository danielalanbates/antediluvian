import bpy,sys,os,glob
src,dst=sys.argv[-2],sys.argv[-1]
for f in sorted(glob.glob(src+'/**/*.fbx',recursive=True)+glob.glob(src+'/**/*.FBX',recursive=True)):
    rel=os.path.relpath(f,src); out=os.path.join(dst,os.path.splitext(rel)[0].replace(' ','_')+'.glb')
    if os.path.exists(out): continue
    os.makedirs(os.path.dirname(out),exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    try: bpy.ops.import_scene.fbx(filepath=f)
    except Exception as e: pass
    meshes=[o for o in bpy.data.objects if o.type=='MESH']
    if not meshes: print('FAIL',f); continue
    bpy.ops.export_scene.gltf(filepath=out,export_format='GLB',export_animations=True)
    print('OK',out)
