"""A test animation for `loom clip` (docs/architecture.md section 19): the
humanoid of humanoid.py with a 2 s action `wave`, exported with it to a
.glb under out/ (not committed). The right arm rises and the forearm waves
twice while the head turns to look at the hand and the hips shift a little;
every keyframe is LINEAR, no Bezier.

Run headless from the repository root:

    blender -b --factory-startup --python tools/blender/humanoid_wave.py -- out/humanoid_wave.glb [--render out/wave_blender 0,30,60]

With --render, Blender also renders the given ticks (frame = tick, 60 fps)
with Workbench to <dir>/blender_<tick>.png, 360x480, from the camera of a
loom scene at (-4.5, 1.0, 0.0) looking at (0.0, 0.9, 0.0) with a 30 degree
vertical field of view, so the two can be compared side by side.
"""

import math
import os
import sys

import bpy
from mathutils import Matrix, Quaternion, Vector

# Import humanoid.py from this directory, without leaving __pycache__ there.
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import humanoid  # noqa: E402

FPS = 60


def turn(axis, degrees):
    """A rotation about an armature-space axis ('X', 'Y' or 'Z')."""
    return Quaternion(Vector({'X': (1, 0, 0), 'Y': (0, 1, 0), 'Z': (0, 0, 1)}[axis]),
                      math.radians(degrees))


# Keyframes: bone -> [(frame, armature-space rotation about the bone's head
# relative to the rest pose, armature-space move)]. Blender's armature axes:
# +X the knight's right, +Y forward, +Z up.
RAISED = turn('Y', -100.0)   # the right arm swung up and out, above the shoulder
LOOK = turn('Z', -35.0)      # the head turned to its right, towards the hand
NONE = Quaternion()
KEYS = {
    'upper_arm_r': [(0, NONE), (30, RAISED), (105, RAISED), (120, NONE)],
    # Two waves of the forearm in the frontal plane, then back.
    'lower_arm_r': [(0, NONE), (30, NONE), (45, turn('Y', 30.0)), (60, turn('Y', -20.0)),
                    (75, turn('Y', 30.0)), (90, turn('Y', -20.0)), (105, NONE), (120, NONE)],
    'head': [(0, NONE), (30, LOOK), (105, LOOK), (120, NONE)],
}
HIPS_MOVE = [(0, Vector((0, 0, 0))), (60, Vector((0.04, 0.0, -0.03))), (120, Vector((0, 0, 0)))]


def add_wave(rig):
    scene = bpy.context.scene
    scene.render.fps = FPS
    scene.frame_start = 0
    scene.frame_end = 120
    rig.animation_data_create()
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode='POSE')
    for name, keys in KEYS.items():
        bone = rig.pose.bones[name]
        bone.rotation_mode = 'QUATERNION'
        rest = bone.bone.matrix_local.to_quaternion()
        for frame, q in keys:
            # The pose rotation is in the bone's rest frame.
            bone.rotation_quaternion = rest.inverted() @ q @ rest
            bone.keyframe_insert('rotation_quaternion', frame=frame)
    hips = rig.pose.bones['hips']
    rest = hips.bone.matrix_local.to_quaternion()
    for frame, move in HIPS_MOVE:
        hips.location = rest.inverted() @ move
        hips.keyframe_insert('location', frame=frame)
    bpy.ops.object.mode_set(mode='OBJECT')
    action = rig.animation_data.action
    action.name = 'wave'
    # keyframe_insert makes Bezier keys: make every one LINEAR.
    count = 0
    for curve in fcurves(action):
        for point in curve.keyframe_points:
            point.interpolation = 'LINEAR'
            count += 1
    print(f'humanoid_wave: {count} keyframes, all LINEAR')


def fcurves(action):
    """Every F-curve of `action`: a layered action's (Blender 4.4 and
    later), else the legacy list."""
    if hasattr(action, 'layers') and len(action.layers) > 0:
        return [curve
                for layer in action.layers
                for strip in layer.strips
                for bag in strip.channelbags
                for curve in bag.fcurves]
    return list(action.fcurves)


def render(out_dir, ticks):
    scene = bpy.context.scene
    camera_data = bpy.data.cameras.new('cam')
    camera_data.sensor_fit = 'VERTICAL'
    camera_data.angle = math.radians(30.0)
    camera = bpy.data.objects.new('cam', camera_data)
    scene.collection.objects.link(camera)
    # loom (-4.5, 1.0, 0.0) looking at (0.0, 0.9, 0.0), in Blender's axes.
    eye, target = Vector((-4.5, 0.0, 1.0)), Vector((0.0, 0.0, 0.9))
    camera.matrix_world = Matrix.Translation(eye) @ (target - eye).to_track_quat('-Z', 'Y').to_matrix().to_4x4()
    scene.camera = camera
    scene.render.engine = 'BLENDER_WORKBENCH'
    scene.display.shading.light = 'STUDIO'
    scene.display.shading.color_type = 'MATERIAL'
    scene.render.resolution_x = 360
    scene.render.resolution_y = 480
    scene.render.resolution_percentage = 100
    world = bpy.data.worlds.new('world')
    world.color = (0.02, 0.02, 0.03)
    scene.world = world
    # Absolute: Blender resolves a relative render path on its own.
    out_dir = os.path.abspath(out_dir)
    os.makedirs(out_dir, exist_ok=True)
    for tick in ticks:
        scene.frame_set(tick)
        scene.render.filepath = os.path.join(out_dir, f'blender_{tick}.png')
        bpy.ops.render.render(write_still=True)


def main():
    args = sys.argv[sys.argv.index('--') + 1:]
    out = args[0]
    rig = humanoid.build()
    add_wave(rig)
    humanoid.export(out, animations=True)
    if '--render' in args:
        i = args.index('--render')
        render(args[i + 1], [int(t) for t in args[i + 2].split(',')])


main()
