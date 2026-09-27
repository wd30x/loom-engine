//! GPU tests of created meshes ([`Renderer::create_mesh`]) and vertex
//! colors: they render a frame and read its pixels back, so they need a
//! GPU and are ignored by `cargo test`; `scripts/smoke.ps1` runs them with
//! the HUD's (`--ignored`).

use glam::{Mat4, Quat, Vec3};

use super::*;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;

/// The RGBA8 pixels of a frame of the draw items `make` gives (with the
/// renderer to create meshes with), seen from the origin down -Z, with
/// `ambient` and `light`.
fn render_with(
    ambient: Vec3,
    light: Option<DirectionalLight>,
    make: impl FnOnce(&mut Renderer) -> Vec<DrawItem>,
) -> Vec<u8> {
    let mut renderer = Renderer::new_headless(WIDTH, HEIGHT).expect("a GPU");
    renderer.resize(WIDTH, HEIGHT).expect("a small size");
    let draws = make(&mut renderer);
    let drawn = renderer
        .render(&Frame {
            view: Mat4::IDENTITY,
            projection: Projection::Perspective { fov_y: 1.0 },
            near: 0.1,
            far: 10.0,
            ambient,
            light,
            draws: &draws,
            sprites: &[],
            hud: &[],
            lines: &[],
        })
        .expect("rendering works");
    assert!(drawn);
    let pixels = renderer.read_pixels().expect("readback works");
    renderer.shutdown();
    pixels
}

/// A linear value encoded as the sRGB target stores it.
fn srgb(linear: f64) -> u8 {
    let s = if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round() as u8
}

#[test]
#[ignore = "needs a GPU; scripts/smoke.ps1 runs it"]
fn created_copy_of_the_built_in_cube_draws_pixel_for_pixel_the_same() {
    let model = Mat4::from_rotation_translation(
        Quat::from_euler(glam::EulerRot::YXZ, 0.6, 0.4, 0.1),
        Vec3::new(0.2, -0.1, -3.0),
    );
    let light = Some(DirectionalLight {
        direction: Vec3::new(-0.3, -1.0, -0.5).normalize(),
        color: Vec3::new(1.0, 0.9, 0.8),
        intensity: 1.0,
    });
    let color = Vec3::new(0.7, 0.4, 0.2);
    let ambient = Vec3::splat(0.2);
    let built_in = render_with(ambient, light, |_| {
        vec![DrawItem {
            mesh: MeshKind::Cube.into(),
            model,
            color,
        }]
    });
    let created = render_with(ambient, light, |r| {
        let (vertices, indices) = mesh::mesh(MeshKind::Cube);
        assert!(vertices.iter().all(|v| v.color == [1.0; 3]));
        let indices: Vec<u32> = indices.into_iter().map(u32::from).collect();
        let id = r.create_mesh(&vertices, &indices).expect("a valid mesh");
        vec![DrawItem {
            mesh: DrawMesh::Id(id),
            model,
            color,
        }]
    });
    // The cube covers part of the frame, lit on several faces.
    let covered = built_in
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|&&p| p != built_in[..4])
        .count();
    assert!(covered > 200, "{covered} pixels");
    assert!(built_in == created, "the created cube differs");
}

#[test]
#[ignore = "needs a GPU; scripts/smoke.ps1 runs it"]
fn vertex_color_times_item_color_is_the_unlit_pixel_color() {
    // A quad facing the camera, 2 in front, covering the frame's center.
    let v = |x: f32, y: f32| Vertex {
        position: [x, y, 0.0],
        normal: [0.0, 0.0, 1.0],
        color: [1.0, 0.5, 0.0],
    };
    let pixels = render_with(Vec3::ONE, None, |r| {
        let id = r
            .create_mesh(
                &[v(-1.0, -1.0), v(1.0, -1.0), v(1.0, 1.0), v(-1.0, 1.0)],
                &[0, 1, 2, 0, 2, 3],
            )
            .expect("a valid mesh");
        vec![DrawItem {
            mesh: DrawMesh::Id(id),
            model: Mat4::from_translation(Vec3::new(0.0, 0.0, -2.0)),
            color: Vec3::new(0.5, 1.0, 1.0),
        }]
    });
    let i = (((HEIGHT / 2) * WIDTH + WIDTH / 2) * 4) as usize;
    assert_eq!(pixels[i..i + 4], [srgb(0.5), srgb(0.5), 0, 255]);
}

#[test]
#[ignore = "needs a GPU; scripts/smoke.ps1 runs it"]
fn create_mesh_rejects_bad_indices_and_render_rejects_a_foreign_mesh() {
    let mut r = Renderer::new_headless(WIDTH, HEIGHT).expect("a GPU");
    r.resize(WIDTH, HEIGHT).expect("a small size");
    let v = Vertex {
        position: [0.0; 3],
        normal: [0.0, 0.0, 1.0],
        color: [1.0; 3],
    };
    let message = |e: RenderError| e.to_string();
    assert_eq!(
        r.create_mesh(&[v; 3], &[0, 1]).map_err(message),
        Err(
            "a mesh needs at least one triangle: 2 indices is not a positive multiple of 3"
                .to_owned()
        )
    );
    assert_eq!(
        r.create_mesh(&[v; 3], &[0, 1, 3]).map_err(message),
        Err("mesh index 3 is out of range: the mesh has 3 vertices".to_owned())
    );
    let foreign = [DrawItem {
        mesh: DrawMesh::Id(MeshId::from_index(0)),
        model: Mat4::IDENTITY,
        color: Vec3::ONE,
    }];
    let frame = Frame {
        view: Mat4::IDENTITY,
        projection: Projection::Perspective { fov_y: 1.0 },
        near: 0.1,
        far: 10.0,
        ambient: Vec3::ONE,
        light: None,
        draws: &foreign,
        sprites: &[],
        hud: &[],
        lines: &[],
    };
    assert_eq!(
        r.render(&frame).map_err(message),
        Err("draw item uses mesh 0, which this renderer did not create".to_owned())
    );
    r.shutdown();
}
