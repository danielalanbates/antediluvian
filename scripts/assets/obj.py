import bpy,sys,os,glob
for f in glob.glob(sys.argv[-1]+'/*.obj'):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.wm.obj_import(filepath=f)
    out='glb/5-low-poly-animals/'+os.path.basename(f)[:-4].replace(' ','_')+'.glb'
    os.makedirs(os.path.dirname(out),exist_ok=True)
    bpy.ops.export_scene.gltf(filepath=out,export_format='GLB'); print('OK',out)
