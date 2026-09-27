"""Generates assets/models/humanoid.glb: a low-poly knight with a sword and
a 19-bone armature, every body part bound whole to one bone.

Run headless from the repository root (docs/architecture.md section 19):

    blender -b --factory-startup --python tools/blender/humanoid.py -- assets/models/humanoid.glb

Everything is built by code from an empty scene. Blender is Z-up with the
character facing +Y, its right hand on +X; the glTF export turns that into
+Y up with the character facing -Z, loom's forward. The rest pose is the
A-pose the parts are modelled in.
"""

import math
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

# ---------------------------------------------------------------- materials

# Linear RGB, exported as each material's baseColorFactor.
MATERIALS = [
    ('armor', (0.32, 0.34, 0.37)),
    ('cloth', (0.45, 0.07, 0.05)),
    ('leather', (0.20, 0.11, 0.05)),
    ('steel', (0.80, 0.82, 0.86)),
    ('visor', (0.03, 0.03, 0.035)),
]
MATERIAL_INDEX = {name: i for i, (name, _) in enumerate(MATERIALS)}

# ---------------------------------------------------------------- skeleton

# Joints in Blender coordinates (metres, Z up, facing +Y).
SHOULDER_Z = 1.46
ARM_DOWN = math.radians(45.0)  # the A-pose: arms 45 degrees below horizontal


def arm_points(side):
    """Shoulder, elbow, wrist and fingertip of one arm; side is -1 (left,
    -X) or +1 (right, +X)."""
    direction = Vector((side * math.cos(ARM_DOWN), 0.0, -math.sin(ARM_DOWN)))
    shoulder = Vector((side * 0.23, 0.0, SHOULDER_Z))
    elbow = shoulder + direction * 0.29
    wrist = elbow + direction * 0.26
    tip = wrist + direction * 0.10
    return shoulder, elbow, wrist, tip


L_SHOULDER, L_ELBOW, L_WRIST, L_TIP = arm_points(-1)
R_SHOULDER, R_ELBOW, R_WRIST, R_TIP = arm_points(+1)

# The sword: held in the right fist, the blade pointing forward and down.
GRIP = (R_WRIST + R_TIP) * 0.5
BLADE_DIR = Vector((0.25, 0.55, -0.80)).normalized()
POMMEL = GRIP - BLADE_DIR * 0.10
GUARD = GRIP + BLADE_DIR * 0.08
SWORD_TIP = GUARD + BLADE_DIR * 0.80


def leg_points(side):
    """Hip, knee, ankle and toe of one leg."""
    hip = Vector((side * 0.10, 0.0, 0.92))
    knee = Vector((side * 0.10, 0.0, 0.50))
    ankle = Vector((side * 0.10, 0.0, 0.09))
    toe = Vector((side * 0.10, 0.17, 0.03))
    return hip, knee, ankle, toe


L_HIP, L_KNEE, L_ANKLE, L_TOE = leg_points(-1)
R_HIP, R_KNEE, R_ANKLE, R_TOE = leg_points(+1)

# (name, parent, head, tail), parents before children.
BONES = [
    ('root', None, Vector((0, 0, 0)), Vector((0, 0.2, 0))),
    ('hips', 'root', Vector((0, 0, 0.92)), Vector((0, 0, 1.05))),
    ('spine', 'hips', Vector((0, 0, 1.05)), Vector((0, 0, 1.22))),
    ('chest', 'spine', Vector((0, 0, 1.22)), Vector((0, 0, 1.50))),
    ('neck', 'chest', Vector((0, 0, 1.50)), Vector((0, 0, 1.58))),
    ('head', 'neck', Vector((0, 0, 1.58)), Vector((0, 0, 1.80))),
    ('upper_arm_l', 'chest', L_SHOULDER, L_ELBOW),
    ('lower_arm_l', 'upper_arm_l', L_ELBOW, L_WRIST),
    ('hand_l', 'lower_arm_l', L_WRIST, L_TIP),
    ('upper_arm_r', 'chest', R_SHOULDER, R_ELBOW),
    ('lower_arm_r', 'upper_arm_r', R_ELBOW, R_WRIST),
    ('hand_r', 'lower_arm_r', R_WRIST, R_TIP),
    ('weapon', 'hand_r', GRIP, SWORD_TIP),
    ('upper_leg_l', 'hips', L_HIP, L_KNEE),
    ('lower_leg_l', 'upper_leg_l', L_KNEE, L_ANKLE),
    ('foot_l', 'lower_leg_l', L_ANKLE, L_TOE),
    ('upper_leg_r', 'hips', R_HIP, R_KNEE),
    ('lower_leg_r', 'upper_leg_r', R_KNEE, R_ANKLE),
    ('foot_r', 'lower_leg_r', R_ANKLE, R_TOE),
]
BONE_INDEX = {name: i for i, (name, *_) in enumerate(BONES)}

# ---------------------------------------------------------------- geometry


def frame_along(a, b):
    """A rotation whose local Z runs from a to b, local X kept as close to
    world X (or world Y for a vertical-ish X) as possible."""
    z = (b - a).normalized()
    ref = Vector((1, 0, 0)) if abs(z.x) < 0.9 else Vector((0, 1, 0))
    x = (ref - z * ref.dot(z)).normalized()
    y = z.cross(x)
    return Matrix((x, y, z)).transposed().to_4x4()


class Body:
    """All the parts in one bmesh: each face has its part's material, each
    vertex its bone's group at weight 1."""

    def __init__(self):
        self.bm = bmesh.new()
        self.deform = self.bm.verts.layers.deform.verify()

    def _add(self, geom_verts, bone, material):
        faces = {f for v in geom_verts for f in v.link_faces}
        for v in geom_verts:
            v[self.deform][BONE_INDEX[bone]] = 1.0
        for f in faces:
            f.material_index = MATERIAL_INDEX[material]
            f.smooth = False

    def box(self, bone, material, center, size, rotation=None):
        """An axis-aligned box of `size` at `center`, or turned by `rotation`
        (a 4x4 matrix) about its center."""
        result = bmesh.ops.create_cube(self.bm, size=1.0)
        m = Matrix.Translation(center)
        if rotation is not None:
            m = m @ rotation
        m = m @ Matrix.Diagonal(Vector((size[0], size[1], size[2], 1.0)))
        bmesh.ops.transform(self.bm, matrix=m, verts=result['verts'])
        self._add(result['verts'], bone, material)

    def beam(self, bone, material, a, b, width, depth):
        """A box from point a to point b, `width` across (local X) and
        `depth` deep (local Y)."""
        length = (b - a).length
        self.box(bone, material, (a + b) * 0.5, (width, depth, length), frame_along(a, b))

    def rod(self, bone, material, a, b, r1, r2, segments=8):
        """A tapered cylinder from a (radius r1) to b (radius r2)."""
        length = (b - a).length
        result = bmesh.ops.create_cone(
            self.bm, cap_ends=True, cap_tris=False, segments=segments,
            radius1=r1, radius2=r2, depth=length)
        m = Matrix.Translation((a + b) * 0.5) @ frame_along(a, b)
        bmesh.ops.transform(self.bm, matrix=m, verts=result['verts'])
        self._add(result['verts'], bone, material)


def build_body():
    body = Body()
    v = Vector

    # Legs: cloth thighs, armored shins, leather boots.
    for side, hip, knee, ankle, toe in [('l', L_HIP, L_KNEE, L_ANKLE, L_TOE),
                                        ('r', R_HIP, R_KNEE, R_ANKLE, R_TOE)]:
        body.rod(f'upper_leg_{side}', 'cloth', hip, knee, 0.085, 0.07)
        body.rod(f'lower_leg_{side}', 'armor', knee, ankle, 0.075, 0.06)
        body.box(f'lower_leg_{side}', 'armor', knee + v((0, 0.035, 0)), (0.12, 0.08, 0.09))
        body.box(f'foot_{side}', 'leather', v((ankle.x, 0.05, 0.045)), (0.12, 0.27, 0.09))

    # Pelvis: leather belt over a cloth skirt, front and back flaps.
    body.box('hips', 'cloth', v((0, 0, 0.97)), (0.34, 0.22, 0.14))
    body.box('hips', 'leather', v((0, 0, 1.05)), (0.36, 0.24, 0.05))
    body.box('hips', 'cloth', v((0, 0.10, 0.76)), (0.30, 0.03, 0.40))
    body.box('hips', 'cloth', v((0, -0.10, 0.76)), (0.30, 0.03, 0.40))

    # Torso: a cloth waist, an armored chest with a red tabard panel.
    body.box('spine', 'cloth', v((0, 0, 1.135)), (0.30, 0.20, 0.17))
    body.box('chest', 'armor', v((0, 0, 1.36)), (0.42, 0.26, 0.28))
    body.box('chest', 'cloth', v((0, 0.135, 1.30)), (0.20, 0.02, 0.30))
    body.box('chest', 'armor', v((0, 0, 1.48)), (0.30, 0.22, 0.05))

    # Neck and helmet with a dark visor slit.
    body.rod('neck', 'leather', v((0, 0, 1.50)), v((0, 0, 1.59)), 0.055, 0.05)
    body.box('head', 'armor', v((0, 0, 1.695)), (0.21, 0.24, 0.21))
    body.box('head', 'visor', v((0, 0.121, 1.70)), (0.15, 0.01, 0.03))
    body.box('head', 'armor', v((0, 0, 1.805)), (0.05, 0.20, 0.03))

    # Arms: pauldrons and bracers of armor, cloth sleeves, leather gloves.
    for side, shoulder, elbow, wrist, tip in [('l', L_SHOULDER, L_ELBOW, L_WRIST, L_TIP),
                                              ('r', R_SHOULDER, R_ELBOW, R_WRIST, R_TIP)]:
        body.box(f'upper_arm_{side}', 'armor', shoulder + v((0, 0, 0.02)), (0.16, 0.18, 0.12),
                 frame_along(shoulder, elbow))
        body.rod(f'upper_arm_{side}', 'cloth', shoulder, elbow, 0.06, 0.05)
        body.rod(f'lower_arm_{side}', 'armor', elbow, wrist, 0.055, 0.045)
        body.beam(f'hand_{side}', 'leather', wrist, tip, 0.07, 0.08)

    # The sword: leather grip, steel pommel, crossguard and blade.
    body.rod('weapon', 'leather', POMMEL, GUARD, 0.022, 0.022, segments=6)
    body.box('weapon', 'steel', POMMEL, (0.04, 0.04, 0.04), frame_along(POMMEL, GUARD))
    body.box('weapon', 'steel', GUARD, (0.20, 0.03, 0.03), frame_along(GUARD, SWORD_TIP))
    body.beam('weapon', 'steel', GUARD, SWORD_TIP, 0.06, 0.02)
    return body


# ---------------------------------------------------------------- scene


def main():
    out = sys.argv[sys.argv.index('--') + 1]

    bpy.ops.wm.read_factory_settings(use_empty=True)

    materials = []
    for name, color in MATERIALS:
        mat = bpy.data.materials.new(name)
        bsdf = mat.node_tree.nodes['Principled BSDF']
        bsdf.inputs['Base Color'].default_value = (*color, 1.0)
        bsdf.inputs['Metallic'].default_value = 0.0
        bsdf.inputs['Roughness'].default_value = 0.8
        materials.append(mat)

    armature = bpy.data.armatures.new('humanoid_skeleton')
    rig = bpy.data.objects.new('humanoid', armature)
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
    mesh = bpy.data.meshes.new('knight')
    body.bm.to_mesh(mesh)
    body.bm.free()
    for mat in materials:
        mesh.materials.append(mat)
    obj = bpy.data.objects.new('knight', mesh)
    bpy.context.scene.collection.objects.link(obj)
    for name, *_ in BONES:
        obj.vertex_groups.new(name=name)
    obj.parent = rig
    modifier = obj.modifiers.new('armature', 'ARMATURE')
    modifier.object = rig

    tris = sum(len(p.vertices) - 2 for p in mesh.polygons)
    print(f'humanoid: {len(mesh.vertices)} vertices, {tris} triangles')

    bpy.ops.export_scene.gltf(
        filepath=out,
        export_format='GLB',
        export_yup=True,
        export_apply=False,
        export_texcoords=False,
        export_normals=True,
        export_tangents=False,
        export_materials='EXPORT',
        export_vertex_color='NONE',
        export_attributes=False,
        export_cameras=False,
        export_lights=False,
        export_extras=False,
        export_animations=False,
        export_skins=True,
        export_morph=False,
        export_def_bones=False,
        export_leaf_bone=False,
        export_rest_position_armature=True,
    )


main()
