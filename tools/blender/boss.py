"""Generates assets/models/boss.glb: a hulking low-poly armored warden, 3 m
tall, with a halberd in its right hand. Its armature has the same 19 bones
as humanoid.py's, by name, parent and order, so the clips' table of axes
(assets/anims/README.md) holds for it too; every body part is bound whole
to one bone.

Run headless from the repository root (docs/architecture.md section 19):

    blender -b --factory-startup --python tools/blender/boss.py -- assets/models/boss.glb

Like humanoid.py: Blender is Z-up with the boss facing +Y, its right hand
on +X; the glTF export turns that into +Y up facing -Z, loom's forward.
The rest pose is the stance the parts are modelled in: arms hanging 55
degrees below horizontal, the halberd upright beside the right side,
leaning a little forward.
"""

import math
import os
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

# Import humanoid.py from this directory, without leaving __pycache__ there.
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import humanoid  # noqa: E402
from humanoid import frame_along  # noqa: E402

# ---------------------------------------------------------------- materials

# Linear RGB, exported as each material's baseColorFactor.
MATERIALS = [
    ('armor', (0.27, 0.25, 0.19)),
    ('cloth', (0.17, 0.06, 0.04)),
    ('leather', (0.14, 0.09, 0.05)),
    ('steel', (0.55, 0.55, 0.52)),
    ('wood', (0.22, 0.14, 0.07)),
    ('hide', (0.30, 0.25, 0.17)),
    ('visor', (0.02, 0.02, 0.02)),
]
MATERIAL_INDEX = {name: i for i, (name, _) in enumerate(MATERIALS)}

# ---------------------------------------------------------------- skeleton

# Joints in Blender coordinates (metres, Z up, facing +Y).
SHOULDER_Z = 2.20
ARM_DOWN = math.radians(55.0)


def arm_points(side):
    """Shoulder, elbow, wrist and fingertip of one arm; side is -1 (left,
    -X) or +1 (right, +X)."""
    direction = Vector((side * math.cos(ARM_DOWN), 0.0, -math.sin(ARM_DOWN)))
    shoulder = Vector((side * 0.40, 0.0, SHOULDER_Z))
    elbow = shoulder + direction * 0.48
    wrist = elbow + direction * 0.42
    tip = wrist + direction * 0.16
    return shoulder, elbow, wrist, tip


L_SHOULDER, L_ELBOW, L_WRIST, L_TIP = arm_points(-1)
R_SHOULDER, R_ELBOW, R_WRIST, R_TIP = arm_points(+1)

# The halberd: held in the right fist a third of the way up its shaft,
# upright and leaning forward, the axe blade facing forward near the top.
GRIP = (R_WRIST + R_TIP) * 0.5
SHAFT_DIR = Vector((0.0, 0.40, 0.92)).normalized()
BUTT = GRIP - SHAFT_DIR * 0.85
HALBERD_TIP = GRIP + SHAFT_DIR * 1.95
BLADE_AT = GRIP + SHAFT_DIR * 1.55


def leg_points(side):
    """Hip, knee, ankle and toe of one leg."""
    hip = Vector((side * 0.18, 0.0, 1.45))
    knee = Vector((side * 0.18, 0.0, 0.78))
    ankle = Vector((side * 0.18, 0.0, 0.14))
    toe = Vector((side * 0.18, 0.30, 0.05))
    return hip, knee, ankle, toe


L_HIP, L_KNEE, L_ANKLE, L_TOE = leg_points(-1)
R_HIP, R_KNEE, R_ANKLE, R_TOE = leg_points(+1)

# (name, parent, head, tail), parents before children: humanoid.py's bones.
BONES = [
    ('root', None, Vector((0, 0, 0)), Vector((0, 0.3, 0))),
    ('hips', 'root', Vector((0, 0, 1.45)), Vector((0, 0, 1.68))),
    ('spine', 'hips', Vector((0, 0, 1.68)), Vector((0, 0, 1.90))),
    ('chest', 'spine', Vector((0, 0, 1.90)), Vector((0, 0, 2.34))),
    ('neck', 'chest', Vector((0, 0, 2.34)), Vector((0, 0, 2.46))),
    ('head', 'neck', Vector((0, 0, 2.46)), Vector((0, 0, 2.84))),
    ('upper_arm_l', 'chest', L_SHOULDER, L_ELBOW),
    ('lower_arm_l', 'upper_arm_l', L_ELBOW, L_WRIST),
    ('hand_l', 'lower_arm_l', L_WRIST, L_TIP),
    ('upper_arm_r', 'chest', R_SHOULDER, R_ELBOW),
    ('lower_arm_r', 'upper_arm_r', R_ELBOW, R_WRIST),
    ('hand_r', 'lower_arm_r', R_WRIST, R_TIP),
    ('weapon', 'hand_r', GRIP, HALBERD_TIP),
    ('upper_leg_l', 'hips', L_HIP, L_KNEE),
    ('lower_leg_l', 'upper_leg_l', L_KNEE, L_ANKLE),
    ('foot_l', 'lower_leg_l', L_ANKLE, L_TOE),
    ('upper_leg_r', 'hips', R_HIP, R_KNEE),
    ('lower_leg_r', 'upper_leg_r', R_KNEE, R_ANKLE),
    ('foot_r', 'lower_leg_r', R_ANKLE, R_TOE),
]
assert [b[:2] for b in BONES] == [b[:2] for b in humanoid.BONES], 'the bones must match humanoid.py'
BONE_INDEX = {name: i for i, (name, *_) in enumerate(BONES)}

# ---------------------------------------------------------------- geometry


class Body(humanoid.Body):
    """humanoid.Body with this file's materials (the bones are the same)."""

    def _add(self, geom_verts, bone, material):
        faces = {f for v in geom_verts for f in v.link_faces}
        for v in geom_verts:
            v[self.deform][BONE_INDEX[bone]] = 1.0
        for f in faces:
            f.material_index = MATERIAL_INDEX[material]
            f.smooth = False


def build_body():
    body = Body()
    v = Vector

    # Legs: cloth thighs, armored shins with knee plates, heavy boots.
    for side, hip, knee, ankle in [('l', L_HIP, L_KNEE, L_ANKLE), ('r', R_HIP, R_KNEE, R_ANKLE)]:
        body.rod(f'upper_leg_{side}', 'cloth', hip, knee, 0.16, 0.13)
        body.rod(f'lower_leg_{side}', 'armor', knee, ankle, 0.135, 0.10)
        body.box(f'lower_leg_{side}', 'armor', knee + v((0, 0.07, 0)), (0.22, 0.12, 0.18))
        body.box(f'foot_{side}', 'leather', v((ankle.x, 0.09, 0.07)), (0.22, 0.44, 0.14))

    # Pelvis: a belt over a cloth skirt, long ragged flaps front and back
    # and shorter ones at the sides.
    body.box('hips', 'cloth', v((0, 0, 1.52)), (0.58, 0.40, 0.24))
    body.box('hips', 'leather', v((0, 0, 1.65)), (0.62, 0.44, 0.09))
    body.box('hips', 'cloth', v((0, 0.19, 1.12)), (0.46, 0.04, 0.78))
    body.box('hips', 'cloth', v((0, -0.19, 1.12)), (0.46, 0.04, 0.78))
    for x in (-0.30, 0.30):
        body.box('hips', 'cloth', v((x, 0, 1.27)), (0.04, 0.32, 0.46))

    # Torso: a cloth waist, a broad breastplate, a gorget, and a hide
    # mantle over the shoulders.
    body.box('spine', 'cloth', v((0, 0, 1.79)), (0.52, 0.36, 0.26))
    body.box('chest', 'armor', v((0, 0, 2.08)), (0.80, 0.48, 0.50))
    body.box('chest', 'armor', v((0, 0.25, 1.98)), (0.56, 0.04, 0.26))
    body.box('chest', 'hide', v((0, -0.02, 2.33)), (0.92, 0.58, 0.12))
    body.box('chest', 'armor', v((0, 0, 2.40)), (0.46, 0.40, 0.08))

    # Neck and a tall helmet with a dark visor slit and a crest.
    body.rod('neck', 'leather', v((0, 0, 2.34)), v((0, 0, 2.47)), 0.10, 0.09)
    body.box('head', 'armor', v((0, 0, 2.63)), (0.34, 0.38, 0.36))
    body.box('head', 'visor', v((0, 0.191, 2.64)), (0.24, 0.01, 0.05))
    body.box('head', 'armor', v((0, 0, 2.89)), (0.07, 0.34, 0.16))

    # Arms: big pauldrons and bracers of armor, cloth sleeves, gauntlets.
    for side, shoulder, elbow, wrist, tip in [('l', L_SHOULDER, L_ELBOW, L_WRIST, L_TIP),
                                              ('r', R_SHOULDER, R_ELBOW, R_WRIST, R_TIP)]:
        body.box(f'upper_arm_{side}', 'armor', shoulder + v((0, 0, 0.04)), (0.34, 0.36, 0.22),
                 frame_along(shoulder, elbow))
        body.rod(f'upper_arm_{side}', 'cloth', shoulder, elbow, 0.10, 0.085)
        body.rod(f'lower_arm_{side}', 'armor', elbow, wrist, 0.095, 0.08)
        body.beam(f'hand_{side}', 'leather', wrist, tip, 0.13, 0.14)

    # The halberd: a wooden shaft with a steel butt cap, an axe blade facing
    # forward, a hook at the back and a spike on top.
    body.rod('weapon', 'wood', BUTT, HALBERD_TIP - SHAFT_DIR * 0.30, 0.035, 0.035, segments=6)
    body.box('weapon', 'steel', BUTT, (0.08, 0.08, 0.10), frame_along(BUTT, HALBERD_TIP))
    across = frame_along(GRIP, HALBERD_TIP)
    # Across the shaft, towards the front: the blade's local Y in `across`.
    forward = SHAFT_DIR.cross(Vector((1, 0, 0))).normalized()
    body.box('weapon', 'steel', BLADE_AT + forward * 0.20, (0.04, 0.38, 0.46), across)
    body.box('weapon', 'steel', BLADE_AT - forward * 0.10, (0.03, 0.16, 0.08), across)
    body.rod('weapon', 'steel', HALBERD_TIP - SHAFT_DIR * 0.30, HALBERD_TIP, 0.05, 0.005,
             segments=6)
    return body


# ---------------------------------------------------------------- scene


def build():
    """Builds the boss in an empty scene; returns the armature object."""
    bpy.ops.wm.read_factory_settings(use_empty=True)

    materials = []
    for name, color in MATERIALS:
        mat = bpy.data.materials.new(name)
        bsdf = mat.node_tree.nodes['Principled BSDF']
        bsdf.inputs['Base Color'].default_value = (*color, 1.0)
        bsdf.inputs['Metallic'].default_value = 0.0
        bsdf.inputs['Roughness'].default_value = 0.8
        materials.append(mat)

    armature = bpy.data.armatures.new('boss_skeleton')
    rig = bpy.data.objects.new('boss', armature)
    bpy.context.scene.collection.objects.link(rig)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode='EDIT')
    for name, parent, head, tail in BONES:
        bone = armature.edit_bones.new(name)
        bone.head = head
        bone.tail = tail
        bone.roll = 0.0
        if parent is not None:
            bone.parent = armature.edit_bones[parent]
    bpy.ops.object.mode_set(mode='OBJECT')

    body = build_body()
    mesh = bpy.data.meshes.new('warden')
    body.bm.to_mesh(mesh)
    body.bm.free()
    for mat in materials:
        mesh.materials.append(mat)
    obj = bpy.data.objects.new('warden', mesh)
    bpy.context.scene.collection.objects.link(obj)
    for name, *_ in BONES:
        obj.vertex_groups.new(name=name)
    obj.parent = rig
    modifier = obj.modifiers.new('armature', 'ARMATURE')
    modifier.object = rig

    tris = sum(len(p.vertices) - 2 for p in mesh.polygons)
    print(f'boss: {len(mesh.vertices)} vertices, {tris} triangles')
    return rig


def main():
    out = sys.argv[sys.argv.index('--') + 1]
    build()
    humanoid.export(out)


if __name__ == '__main__':
    main()
