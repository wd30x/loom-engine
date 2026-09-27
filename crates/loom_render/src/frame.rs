//! What the caller hands the renderer each frame, and how it is packed for
//! the GPU. The renderer knows nothing about scenes: the engine converts its
//! scene into a [`Frame`].

use bytemuck::{Pod, Zeroable};
use glam::camera::rh;
use glam::{Mat3, Mat4, Vec2, Vec3, Vec4};

use std::ops::Range;

use crate::{MeshKind, TextureId};

/// Everything needed to draw one frame.
#[derive(Debug, Clone, Copy)]
pub struct Frame<'a> {
    /// World to view space: the inverse of the camera's world matrix.
    pub view: Mat4,
    pub projection: Projection,
    pub near: f32,
    pub far: f32,
    /// Ambient light, linear RGB.
    pub ambient: Vec3,
    pub light: Option<DirectionalLight>,
    /// Drawn in this order.
    pub draws: &'a [DrawItem],
    /// Drawn in this order, after `draws`, in the same pass and against the
    /// same depth buffer. Unlit and alpha-tested, so the order does not
    /// change the result.
    pub sprites: &'a [SpriteDraw],
    /// The HUD, in pixels, drawn after `sprites` and before `lines` in this
    /// order: alpha-blended, no depth test. Nothing is drawn for an empty
    /// list.
    pub hud: &'a [HudDraw],
    /// Debug lines, drawn last in this order, over everything: no depth
    /// test. Nothing is drawn for an empty list.
    pub lines: &'a [LineDraw],
}

/// One HUD item in pixels of the render target (origin at the top left,
/// +Y down). Colors are linear RGB plus alpha, blended over what is below.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HudDraw {
    /// A filled axis-aligned rectangle: its top-left corner and its size.
    Rect { min: Vec2, size: Vec2, color: Vec4 },
    /// A filled disc: a pixel is covered when its center is within
    /// `radius` of `center` (a hard edge).
    Disc {
        center: Vec2,
        radius: f32,
        color: Vec4,
    },
    /// The pixels `rect` (x, y, width, height; inside the texture) of
    /// `texture` stretched over `min`..`min + size`, unfiltered: texels with
    /// alpha < 0.5 are left out, the rest are their color times `color`,
    /// with `color`'s alpha.
    Image {
        texture: TextureId,
        min: Vec2,
        size: Vec2,
        rect: [u32; 4],
        color: Vec4,
    },
}

/// One debug line segment in world space, 1 pixel wide, opaque.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineDraw {
    pub a: Vec3,
    pub b: Vec3,
    /// Linear RGB.
    pub color: Vec3,
}

/// How view space maps to clip space. Both look along -Z with depth 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Projection {
    /// Vertical field of view in radians.
    Perspective { fov_y: f32 },
    /// `height` is the view-space length the view covers vertically; the
    /// width is `height * aspect`, centered on the view axis.
    Orthographic { height: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirectionalLight {
    /// World-space direction the light travels in, normalized.
    pub direction: Vec3,
    /// Linear RGB.
    pub color: Vec3,
    pub intensity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawItem {
    pub mesh: MeshKind,
    /// Object to world.
    pub model: Mat4,
    /// Linear RGB.
    pub color: Vec3,
}

/// A 1x1 quad on the XY plane of `model`, centered at its origin and facing
/// +Z, showing the pixels `rect` of `texture` unfiltered: uv (0, 0) is the
/// quad's top-left corner (-0.5, 0.5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteDraw {
    pub texture: TextureId,
    /// Object to world.
    pub model: Mat4,
    /// x, y, width, height in texture pixels; must lie inside the texture.
    pub rect: [u32; 4],
}

/// World to clip space for a camera at `view` (world to view) with
/// `projection`, at the render size's `aspect`: what the renderer uses.
pub fn view_projection(
    view: Mat4,
    projection: Projection,
    aspect: f32,
    near: f32,
    far: f32,
) -> Mat4 {
    self::projection(projection, aspect, near, far) * view
}

/// Right-handed projection with depth 0..1. The perspective one is the
/// non-deprecated name of glam's `Mat4::perspective_rh`.
pub(crate) fn projection(projection: Projection, aspect: f32, near: f32, far: f32) -> Mat4 {
    match projection {
        Projection::Perspective { fov_y } => {
            rh::proj::directx::perspective(fov_y, aspect, near, far)
        }
        Projection::Orthographic { height } => {
            let (half_w, half_h) = (height * aspect / 2.0, height / 2.0);
            rh::proj::directx::orthographic(-half_w, half_w, -half_h, half_h, near, far)
        }
    }
}

/// Per-frame uniforms; matches `Globals` in `shaders/mesh.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuGlobals {
    view_proj: [[f32; 4]; 4],
    ambient: [f32; 4],
    /// xyz: direction the light travels in.
    light_dir: [f32; 4],
    /// rgb: color * intensity; zero without a light.
    light_color: [f32; 4],
}

impl GpuGlobals {
    pub fn new(frame: &Frame<'_>, aspect: f32) -> Self {
        let view_proj =
            view_projection(frame.view, frame.projection, aspect, frame.near, frame.far);
        let (dir, color) = match frame.light {
            Some(l) => (l.direction, l.color * l.intensity),
            None => (Vec3::NEG_Y, Vec3::ZERO),
        };
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            ambient: frame.ambient.extend(0.0).to_array(),
            light_dir: dir.extend(0.0).to_array(),
            light_color: color.extend(0.0).to_array(),
        }
    }
}

/// One draw item; matches `Draw` in `shaders/mesh.wgsl` (128 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuDraw {
    model: [[f32; 4]; 4],
    /// Inverse transpose of the model's upper 3x3, as three padded columns,
    /// so normals stay perpendicular under non-uniform scale.
    normal: [[f32; 4]; 3],
    color: [f32; 4],
}

impl GpuDraw {
    pub fn new(item: &DrawItem) -> Self {
        let n = Mat3::from_mat4(item.model).inverse().transpose();
        Self {
            model: item.model.to_cols_array_2d(),
            normal: [n.x_axis, n.y_axis, n.z_axis].map(|c| c.extend(0.0).to_array()),
            color: item.color.extend(1.0).to_array(),
        }
    }
}

/// One sprite; matches `Sprite` in `shaders/sprite.wgsl` (80 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuSprite {
    model: [[f32; 4]; 4],
    rect: [u32; 4],
}

impl GpuSprite {
    pub fn new(sprite: &SpriteDraw) -> Self {
        Self {
            model: sprite.model.to_cols_array_2d(),
            rect: sprite.rect,
        }
    }
}

/// One end of a debug line; matches `VertexIn` in `shaders/line.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct LineVertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl LineVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LineVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// The vertices of `lines` for a line list: both ends of each, in order.
pub(crate) fn line_vertices(lines: &[LineDraw]) -> Vec<LineVertex> {
    lines
        .iter()
        .flat_map(|l| {
            let color = l.color.to_array();
            [l.a, l.b].map(|p| LineVertex {
                position: p.to_array(),
                color,
            })
        })
        .collect()
}

/// One corner of a HUD item; matches `VertexIn` in `shaders/hud.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct HudVertex {
    /// NDC.
    position: [f32; 2],
    /// -1..1 across a disc's bounding square; 0 on anything else.
    local: [f32; 2],
    color: [f32; 4],
    /// 0 at the quad's top-left corner, 1 at the bottom-right.
    uv: [f32; 2],
    /// An image's x, y, width, height in texture pixels; 0 on the rest.
    rect: [u32; 4],
}

impl HudVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x2, 4 => Uint32x4
    ];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<HudVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// A run of HUD vertices drawn alike: solid (`None`) or with one texture.
pub(crate) type HudRun = (Option<TextureId>, Range<u32>);

/// The vertices of `hud` for a triangle list on a `width` x `height` pixel
/// target, two triangles per item in order (a disc is its bounding square),
/// and the runs to draw them in: consecutive items drawn alike (solid, or
/// the same texture) share a run, so drawing the runs in order keeps the
/// list's order.
pub(crate) fn hud_vertices(
    hud: &[HudDraw],
    width: u32,
    height: u32,
) -> (Vec<HudVertex>, Vec<HudRun>) {
    let (w, h) = (width as f32, height as f32);
    let ndc = |p: Vec2| [p.x / w * 2.0 - 1.0, 1.0 - p.y / h * 2.0];
    let mut vertices = Vec::with_capacity(hud.len() * 6);
    let mut runs: Vec<HudRun> = Vec::new();
    for item in hud {
        let (min, max, disc, color, texture, rect) = match *item {
            HudDraw::Rect { min, size, color } => (min, min + size, false, color, None, [0; 4]),
            HudDraw::Disc {
                center,
                radius,
                color,
            } => (
                center - Vec2::splat(radius),
                center + Vec2::splat(radius),
                true,
                color,
                None,
                [0; 4],
            ),
            HudDraw::Image {
                texture,
                min,
                size,
                rect,
                color,
            } => (min, min + size, false, color, Some(texture), rect),
        };
        let corner = |x: bool, y: bool| {
            let p = Vec2::new(if x { max.x } else { min.x }, if y { max.y } else { min.y });
            let local = if disc {
                [if x { 1.0 } else { -1.0 }, if y { 1.0 } else { -1.0 }]
            } else {
                [0.0, 0.0]
            };
            HudVertex {
                position: ndc(p),
                local,
                color: color.to_array(),
                uv: [f32::from(u8::from(x)), f32::from(u8::from(y))],
                rect,
            }
        };
        let (a, b, c, d) = (
            corner(false, false),
            corner(true, false),
            corner(true, true),
            corner(false, true),
        );
        let first = u32::try_from(vertices.len()).expect("hud vertex count fits in u32");
        vertices.extend([a, b, c, a, c, d]);
        match runs.last_mut() {
            Some((t, range)) if *t == texture => range.end = first + 6,
            _ => runs.push((texture, first..first + 6)),
        }
    }
    (vertices, runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_vertices_are_two_triangles_per_item_in_ndc() {
        let hud = [
            HudDraw::Rect {
                min: Vec2::new(0.0, 0.0),
                size: Vec2::new(100.0, 50.0),
                color: Vec4::new(1.0, 0.0, 0.0, 0.5),
            },
            HudDraw::Disc {
                center: Vec2::new(100.0, 50.0),
                radius: 25.0,
                color: Vec4::ONE,
            },
            HudDraw::Image {
                texture: TextureId(3),
                min: Vec2::new(150.0, 50.0),
                size: Vec2::new(50.0, 50.0),
                rect: [16, 0, 16, 16],
                color: Vec4::ONE,
            },
        ];
        let (v, runs) = hud_vertices(&hud, 200, 100);
        assert_eq!(v.len(), 18);
        // The rectangle: the top-left quarter of the screen, local 0.
        assert_eq!(v[0].position, [-1.0, 1.0]);
        assert_eq!(v[2].position, [0.0, 0.0]);
        assert_eq!(v[5].position, [-1.0, 0.0]);
        assert!(
            v[..6]
                .iter()
                .all(|v| v.local == [0.0, 0.0] && v.rect == [0; 4])
        );
        assert_eq!(v[0].color, [1.0, 0.0, 0.0, 0.5]);
        // The disc: its bounding square around the center, local ±1.
        assert_eq!((v[6].position, v[6].local), ([-0.25, 0.5], [-1.0, -1.0]));
        assert_eq!((v[8].position, v[8].local), ([0.25, -0.5], [1.0, 1.0]));
        // The image: uv from the top-left corner, its rect on every corner.
        assert_eq!((v[12].position, v[12].uv), ([0.5, 0.0], [0.0, 0.0]));
        assert_eq!((v[14].position, v[14].uv), ([1.0, -1.0], [1.0, 1.0]));
        assert!(v[12..].iter().all(|v| v.rect == [16, 0, 16, 16]));
        // 56 bytes a vertex, as the shader's five attributes.
        assert_eq!(bytemuck::cast_slice::<_, u8>(&v).len(), 18 * 56);
        // The two solid items share a run; the image has its own.
        assert_eq!(runs, [(None, 0..12), (Some(TextureId(3)), 12..18)]);
        assert_eq!(hud_vertices(&[], 200, 100), (Vec::new(), Vec::new()));
    }

    #[test]
    fn hud_runs_split_wherever_the_kind_or_texture_changes() {
        let rect = HudDraw::Rect {
            min: Vec2::ZERO,
            size: Vec2::ONE,
            color: Vec4::ONE,
        };
        let image = |t| HudDraw::Image {
            texture: TextureId(t),
            min: Vec2::ZERO,
            size: Vec2::ONE,
            rect: [0, 0, 1, 1],
            color: Vec4::ONE,
        };
        let hud = [image(1), image(1), rect, image(1), image(2), rect, rect];
        let (_, runs) = hud_vertices(&hud, 10, 10);
        assert_eq!(
            runs,
            [
                (Some(TextureId(1)), 0..12),
                (None, 12..18),
                (Some(TextureId(1)), 18..24),
                (Some(TextureId(2)), 24..30),
                (None, 30..42),
            ]
        );
    }

    #[test]
    fn line_vertices_are_both_ends_of_each_line_with_its_color() {
        let lines = [
            LineDraw {
                a: Vec3::new(1.0, 2.0, 3.0),
                b: Vec3::new(4.0, 5.0, 6.0),
                color: Vec3::new(1.0, 0.0, 0.0),
            },
            LineDraw {
                a: Vec3::ZERO,
                b: Vec3::Y,
                color: Vec3::new(0.2, 1.0, 0.2),
            },
        ];
        let v = line_vertices(&lines);
        assert_eq!(v.len(), 4);
        assert_eq!(
            (v[0].position, v[0].color),
            ([1.0, 2.0, 3.0], [1.0, 0.0, 0.0])
        );
        assert_eq!(
            (v[1].position, v[1].color),
            ([4.0, 5.0, 6.0], [1.0, 0.0, 0.0])
        );
        assert_eq!(
            (v[3].position, v[3].color),
            ([0.0, 1.0, 0.0], [0.2, 1.0, 0.2])
        );
        // 24 bytes a vertex, as the shader's two vec3 attributes.
        assert_eq!(bytemuck::cast_slice::<_, u8>(&v).len(), 4 * 24);
        assert!(line_vertices(&[]).is_empty());
    }

    #[test]
    #[allow(deprecated)]
    fn projection_matches_the_deprecated_glam_name() {
        let (fov, aspect, near, far) = (1.0, 16.0 / 9.0, 0.1, 100.0);
        assert_eq!(
            projection(Projection::Perspective { fov_y: fov }, aspect, near, far),
            Mat4::perspective_rh(fov, aspect, near, far)
        );
    }

    #[test]
    fn orthographic_projection_maps_the_view_box_to_the_ndc_corners() {
        let (height, aspect, near, far) = (11.25, 16.0 / 9.0, 0.1, 100.0);
        let proj = projection(Projection::Orthographic { height }, aspect, near, far);
        let (half_w, half_h) = (height * aspect / 2.0, height / 2.0);
        for (depth, ndc_z) in [(near, 0.0), (far, 1.0)] {
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let p = proj.project_point3(Vec3::new(sx * half_w, sy * half_h, -depth));
                let expected = Vec3::new(sx, sy, ndc_z);
                assert!(
                    (p - expected).abs().max_element() < 1e-5,
                    "{p} != {expected}"
                );
            }
        }
    }

    #[test]
    fn normal_matrix_keeps_normals_perpendicular_under_non_uniform_scale() {
        let model = Mat4::from_scale(Vec3::new(4.0, 1.0, 1.0))
            * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_4);
        let draw = GpuDraw::new(&DrawItem {
            mesh: MeshKind::Cube,
            model,
            color: Vec3::ONE,
        });
        let n = Mat3::from_cols_array_2d(&draw.normal.map(|c| [c[0], c[1], c[2]]));
        // A surface through the origin spanned by tangent t, with normal nrm.
        let (t, nrm) = (Vec3::new(1.0, 1.0, 0.0), Vec3::new(1.0, -1.0, 0.0));
        let world_t = Mat3::from_mat4(model) * t;
        assert!((n * nrm).dot(world_t).abs() < 1e-5);
    }
}
