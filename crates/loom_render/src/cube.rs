//! Unit cube mesh: 1 m edge, centered at the origin, one flat color per face.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

impl Vertex {
    pub const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// One face: outward normal, two in-plane axes with `u x v == normal`, and
/// its linear color.
struct Face {
    normal: Vec3,
    u: Vec3,
    v: Vec3,
    color: Vec3,
}

const FACES: [Face; 6] = [
    // +X red
    Face {
        normal: Vec3::X,
        u: Vec3::NEG_Z,
        v: Vec3::Y,
        color: Vec3::new(1.0, 0.0, 0.0),
    },
    // -X cyan
    Face {
        normal: Vec3::NEG_X,
        u: Vec3::Z,
        v: Vec3::Y,
        color: Vec3::new(0.0, 1.0, 1.0),
    },
    // +Y green
    Face {
        normal: Vec3::Y,
        u: Vec3::X,
        v: Vec3::NEG_Z,
        color: Vec3::new(0.0, 1.0, 0.0),
    },
    // -Y magenta
    Face {
        normal: Vec3::NEG_Y,
        u: Vec3::X,
        v: Vec3::Z,
        color: Vec3::new(1.0, 0.0, 1.0),
    },
    // +Z blue
    Face {
        normal: Vec3::Z,
        u: Vec3::X,
        v: Vec3::Y,
        color: Vec3::new(0.0, 0.0, 1.0),
    },
    // -Z yellow
    Face {
        normal: Vec3::NEG_Z,
        u: Vec3::NEG_X,
        v: Vec3::Y,
        color: Vec3::new(1.0, 1.0, 0.0),
    },
];

/// Returns 24 vertices (4 per face, so each face keeps its own color) and 36
/// indices. Triangles wind counter-clockwise seen from outside the cube.
pub(crate) fn cube_mesh() -> (Vec<Vertex>, Vec<u16>) {
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for face in &FACES {
        let base = u16::try_from(vertices.len()).expect("cube has 24 vertices");
        let center = face.normal * 0.5;
        // Counter-clockwise around the normal because u x v == normal.
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = center + (face.u * su + face.v * sv) * 0.5;
            vertices.push(Vertex {
                position: p.to_array(),
                color: face.color.to_array(),
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_have_right_handed_axes() {
        for face in &FACES {
            assert_eq!(face.u.cross(face.v), face.normal);
        }
    }

    #[test]
    fn mesh_is_unit_cube_wound_outward() {
        let (vertices, indices) = cube_mesh();
        assert_eq!(vertices.len(), 24);
        assert_eq!(indices.len(), 36);
        for v in &vertices {
            for c in v.position {
                assert_eq!(c.abs(), 0.5);
            }
        }
        for tri in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(vertices[usize::from(tri[i])].position));
            let normal = (b - a).cross(c - a);
            let centroid = (a + b + c) / 3.0;
            assert!(normal.dot(centroid) > 0.0, "triangle {tri:?} faces inward");
        }
    }
}
