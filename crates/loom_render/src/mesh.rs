//! Built-in unit meshes with normals. Triangles wind counter-clockwise seen
//! from the front (the side the normals point to).

use std::f32::consts::{PI, TAU};

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

/// The meshes the renderer can draw. The first three are unit sized; scale
/// them with the model matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MeshKind {
    /// Edge 1, centered at the origin.
    Cube,
    /// 1x1 on the XZ plane, facing +Y. Invisible from below.
    Plane,
    /// Diameter 1, centered at the origin.
    Sphere,
    /// A cylinder along Y capped by two half spheres. `height` includes the
    /// caps; the origin is at the bottom, so it spans y in `[0, height]`.
    /// The caller guarantees finite values, `radius > 0` and
    /// `height >= 2 * radius`.
    Capsule { radius: f32, height: f32 },
}

impl MeshKind {
    /// The meshes uploaded when the renderer is created; others (capsules)
    /// are uploaded the first time a frame uses them.
    pub(crate) const BUILT_IN: [MeshKind; 3] = [MeshKind::Cube, MeshKind::Plane, MeshKind::Sphere];
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
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

    fn new(position: Vec3, normal: Vec3) -> Self {
        Self {
            position: position.to_array(),
            normal: normal.to_array(),
        }
    }
}

pub(crate) fn mesh(kind: MeshKind) -> (Vec<Vertex>, Vec<u16>) {
    match kind {
        MeshKind::Cube => cube(),
        MeshKind::Plane => plane(),
        MeshKind::Sphere => sphere(32, 16),
        MeshKind::Capsule { radius, height } => capsule(radius, height, 32, 8),
    }
}

/// Appends a quad with corners `center ± u/2 ± v/2`, facing `u x v`.
fn quad(vertices: &mut Vec<Vertex>, indices: &mut Vec<u16>, center: Vec3, u: Vec3, v: Vec3) {
    let base = u16::try_from(vertices.len()).expect("built-in meshes have few vertices");
    let normal = u.cross(v).normalize();
    for (su, sv) in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
        vertices.push(Vertex::new(center + u * su + v * sv, normal));
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// 24 vertices (4 per face, so each face has its own normal), 36 indices.
fn cube() -> (Vec<Vertex>, Vec<u16>) {
    let (mut vertices, mut indices) = (Vec::with_capacity(24), Vec::with_capacity(36));
    // (outward normal, u, v) with u x v == normal.
    for (n, u, v) in [
        (Vec3::X, Vec3::NEG_Z, Vec3::Y),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, Vec3::NEG_Z),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
    ] {
        quad(&mut vertices, &mut indices, n * 0.5, u, v);
    }
    (vertices, indices)
}

fn plane() -> (Vec<Vertex>, Vec<u16>) {
    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    quad(
        &mut vertices,
        &mut indices,
        Vec3::ZERO,
        Vec3::X,
        Vec3::NEG_Z,
    );
    (vertices, indices)
}

/// UV sphere of diameter 1: `segments` around (longitude), `rings` from
/// top to bottom (latitude). The seam and pole vertices are duplicated so
/// every ring has `segments + 1` vertices.
fn sphere(segments: u16, rings: u16) -> (Vec<Vertex>, Vec<u16>) {
    let mut vertices = Vec::new();
    for i in 0..=rings {
        let theta = PI * f32::from(i) / f32::from(rings); // 0 at the top
        for j in 0..=segments {
            let phi = TAU * f32::from(j) / f32::from(segments);
            let n = Vec3::new(
                theta.sin() * phi.cos(),
                theta.cos(),
                theta.sin() * phi.sin(),
            );
            vertices.push(Vertex::new(n * 0.5, n));
        }
    }
    let at = |i: u16, j: u16| i * (segments + 1) + j;
    let mut indices = Vec::new();
    for i in 0..rings {
        for j in 0..segments {
            // a d
            // b c   (i grows downwards, j grows towards +Z around the axis)
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
            if i != 0 {
                indices.extend_from_slice(&[a, d, c]); // degenerate at the top pole
            }
            if i != rings - 1 {
                indices.extend_from_slice(&[a, c, b]); // degenerate at the bottom pole
            }
        }
    }
    (vertices, indices)
}

/// Capsule along Y from y = 0 to y = `height`: two half spheres of `radius`
/// (`rings` rings each, `segments` around) joined by a cylinder. The top
/// half sphere is centered at `height - radius`, the bottom one at `radius`;
/// the cylinder reuses their equator rows, whose normals are horizontal.
/// With `height == 2 * radius` the cylinder is left out (a sphere).
fn capsule(radius: f32, height: f32, segments: u16, rings: u16) -> (Vec<Vertex>, Vec<u16>) {
    debug_assert!(radius.is_finite() && height.is_finite());
    debug_assert!(radius > 0.0 && height >= 2.0 * radius);
    let mut vertices = Vec::new();
    // Rows from the top pole down; theta is the angle from +Y.
    for i in 0..=2 * rings + 1 {
        let top = i <= rings;
        let step = if top { i } else { i - 1 };
        let theta = PI * f32::from(step) / f32::from(2 * rings);
        let (sin_theta, cos_theta) = theta.sin_cos();
        // Written so that the poles land exactly on y = height and y = 0.
        let y = if top {
            height - radius * (1.0 - cos_theta)
        } else {
            radius * (1.0 + cos_theta)
        };
        for j in 0..=segments {
            let phi = TAU * f32::from(j) / f32::from(segments);
            let n = Vec3::new(sin_theta * phi.cos(), cos_theta, sin_theta * phi.sin()).normalize();
            let position = Vec3::new(n.x * radius, y, n.z * radius);
            vertices.push(Vertex::new(position, n));
        }
    }
    let rows = 2 * rings + 2;
    let at = |i: u16, j: u16| i * (segments + 1) + j;
    let mut indices = Vec::new();
    for i in 0..rows - 1 {
        if i == rings && height == 2.0 * radius {
            continue; // no cylinder: the two equator rows coincide
        }
        for j in 0..segments {
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
            if i != 0 {
                indices.extend_from_slice(&[a, d, c]); // degenerate at the top pole
            }
            if i != rows - 2 {
                indices.extend_from_slice(&[a, c, b]); // degenerate at the bottom pole
            }
        }
    }
    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangles(kind: MeshKind) -> Vec<[Vertex; 3]> {
        let (vertices, indices) = mesh(kind);
        indices
            .chunks(3)
            .map(|t| [0, 1, 2].map(|k| vertices[usize::from(t[k])]))
            .collect()
    }

    /// Every triangle's winding normal points away from `outward(centroid)`
    /// and agrees with its three vertex normals.
    fn assert_wound_outward(kind: MeshKind, outward: impl Fn(Vec3) -> Vec3) {
        for tri in triangles(kind) {
            let [a, b, c] = tri.map(|v| Vec3::from(v.position));
            let face = (b - a).cross(c - a);
            assert!(
                face.length() > 0.0,
                "{kind:?}: degenerate triangle {a} {b} {c}"
            );
            let centroid = (a + b + c) / 3.0;
            assert!(
                face.dot(outward(centroid)) > 0.0,
                "{kind:?}: {a} {b} {c} faces inward"
            );
            for v in tri {
                assert!(
                    face.dot(Vec3::from(v.normal)) > 0.0,
                    "{kind:?}: normal disagrees"
                );
            }
        }
    }

    #[test]
    fn cube_triangles_face_outward_along_their_normals() {
        assert_wound_outward(MeshKind::Cube, |c| c);
        let (vertices, indices) = mesh(MeshKind::Cube);
        assert_eq!((vertices.len(), indices.len()), (24, 36));
        assert!(
            vertices
                .iter()
                .all(|v| v.position.iter().all(|c| c.abs() == 0.5))
        );
    }

    #[test]
    fn plane_lies_in_xz_and_faces_up() {
        assert_wound_outward(MeshKind::Plane, |_| Vec3::Y);
        let (vertices, _) = mesh(MeshKind::Plane);
        for v in vertices {
            assert_eq!(v.position[1], 0.0);
            assert_eq!(v.normal, [0.0, 1.0, 0.0]);
            assert!(v.position[0].abs() == 0.5 && v.position[2].abs() == 0.5);
        }
    }

    /// Outward reference for a capsule: away from the nearest point of its
    /// axis segment.
    fn capsule_outward(radius: f32, height: f32) -> impl Fn(Vec3) -> Vec3 {
        move |c| c - Vec3::new(0.0, c.y.clamp(radius, height - radius), 0.0)
    }

    fn assert_capsule_shape(radius: f32, height: f32) {
        let kind = MeshKind::Capsule { radius, height };
        assert_wound_outward(kind, capsule_outward(radius, height));
        let (vertices, _) = mesh(kind);
        let (mut lowest, mut highest) = (f32::INFINITY, f32::NEG_INFINITY);
        for v in &vertices {
            let [x, y, z] = v.position;
            // sin² + cos² may exceed 1 by an ulp or so.
            assert!(x * x + z * z <= radius * radius * (1.0 + 1e-6), "{x} {z}");
            assert!((0.0..=height).contains(&y), "{y}");
            assert!((Vec3::from(v.normal).length() - 1.0).abs() < 1e-6);
            lowest = lowest.min(y);
            highest = highest.max(y);
        }
        assert_eq!((lowest, highest), (0.0, height));
    }

    #[test]
    fn capsule_triangles_face_away_from_its_axis() {
        assert_capsule_shape(0.35, 1.8);
        let (vertices, indices) = mesh(MeshKind::Capsule {
            radius: 0.35,
            height: 1.8,
        });
        assert_eq!(vertices.len(), 18 * 33);
        // Two pole bands of 32 triangles, 15 full bands of 64 (one is the cylinder).
        assert_eq!(indices.len(), (2 * 32 + 15 * 64) * 3);
    }

    #[test]
    fn capsule_as_tall_as_its_diameter_is_a_sphere() {
        assert_capsule_shape(0.5, 1.0);
        let (_, indices) = mesh(MeshKind::Capsule {
            radius: 0.5,
            height: 1.0,
        });
        // As above, without the cylinder band.
        assert_eq!(indices.len(), (2 * 32 + 14 * 64) * 3);
    }

    #[test]
    fn sphere_triangles_face_outward_along_their_normals() {
        assert_wound_outward(MeshKind::Sphere, |c| c);
        let (vertices, indices) = mesh(MeshKind::Sphere);
        assert_eq!(vertices.len(), 17 * 33);
        assert_eq!(indices.len(), (2 * 32 + 14 * 64) * 3);
        for v in vertices {
            assert!((Vec3::from(v.position).length() - 0.5).abs() < 1e-6);
        }
    }
}
