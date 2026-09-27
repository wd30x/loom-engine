//! GPU tests of created meshes ([`Renderer::create_mesh`]), skinned
//! meshes ([`Renderer::create_skinned_mesh`]) and vertex colors: they
//! render a frame and read its pixels back, so they need a GPU and are
//! ignored by `cargo test`; `scripts/smoke.ps1` runs them with the HUD's
//! (`--ignored`).

use glam::{Mat4, Quat, Vec3};

use super::*;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;

/// The RGBA8 pixels of a frame of the draw items `make` gives (with the
/// renderer to create meshes with) and the joint matrices `joints`, seen
/// from the origin down -Z, with `ambient` and `light`.
fn render_with(
    ambient: Vec3,
    light: Option<DirectionalLight>,
    joints: &[Mat4],
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
            joints,
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
    let built_in = render_with(ambient, light, &[], |_| {
        vec![DrawItem {
            mesh: MeshKind::Cube.into(),
            model,
            color,
            joints: None,
        }]
    });
    let created = render_with(ambient, light, &[], |r| {
        let (vertices, indices) = mesh::mesh(MeshKind::Cube);
        assert!(vertices.iter().all(|v| v.color == [1.0; 3]));
        let indices: Vec<u32> = indices.into_iter().map(u32::from).collect();
        let id = r.create_mesh(&vertices, &indices).expect("a valid mesh");
        vec![DrawItem {
            mesh: DrawMesh::Id(id),
            model,
            color,
            joints: None,
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
    let pixels = render_with(Vec3::ONE, None, &[], |r| {
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
            joints: None,
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
        joints: None,
    }];
    let frame = Frame {
        view: Mat4::IDENTITY,
        projection: Projection::Perspective { fov_y: 1.0 },
        near: 0.1,
        far: 10.0,
        ambient: Vec3::ONE,
        light: None,
        draws: &foreign,
        joints: &[],
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

/// A turned cube 3 in front of the camera, lit from the upper left.
fn cube_model() -> Mat4 {
    Mat4::from_rotation_translation(
        Quat::from_euler(glam::EulerRot::YXZ, 0.6, 0.4, 0.1),
        Vec3::new(0.2, -0.1, -3.0),
    )
}

fn cube_light() -> Option<DirectionalLight> {
    Some(DirectionalLight {
        direction: Vec3::new(-0.3, -1.0, -0.5).normalize(),
        color: Vec3::new(1.0, 0.9, 0.8),
        intensity: 1.0,
    })
}

const CUBE_COLOR: Vec3 = Vec3::new(0.7, 0.4, 0.2);
const CUBE_AMBIENT: Vec3 = Vec3::splat(0.2);

/// The built-in cube at `model`.
fn built_in_cube(model: Mat4) -> Vec<u8> {
    render_with(CUBE_AMBIENT, cube_light(), &[], |_| {
        vec![DrawItem {
            mesh: MeshKind::Cube.into(),
            model,
            color: CUBE_COLOR,
            joints: None,
        }]
    })
}

/// The built-in cube's vertices as a skinned mesh, every vertex bound to
/// joints `joints` with `weights`, drawn at `model` with the joint
/// matrices `matrices` (all of them its range).
fn skinned_cube(model: Mat4, joints: [u16; 4], weights: [f32; 4], matrices: &[Mat4]) -> Vec<u8> {
    render_with(CUBE_AMBIENT, cube_light(), matrices, |r| {
        let (vertices, indices) = mesh::mesh(MeshKind::Cube);
        let vertices: Vec<SkinnedVertex> = vertices
            .iter()
            .map(|v| SkinnedVertex {
                position: v.position,
                normal: v.normal,
                color: v.color,
                joints,
                weights,
            })
            .collect();
        let indices: Vec<u32> = indices.into_iter().map(u32::from).collect();
        let id = r
            .create_skinned_mesh(&vertices, &indices)
            .expect("a valid mesh");
        vec![DrawItem {
            mesh: DrawMesh::Id(id),
            model,
            color: CUBE_COLOR,
            joints: Some(JointRange {
                first: 0,
                count: u32::try_from(matrices.len()).expect("a few"),
            }),
        }]
    })
}

/// How many pixels differ between two frames, and by how much at most
/// in any channel.
fn difference(a: &[u8], b: &[u8]) -> (usize, u8) {
    let differ = a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .filter(|(p, q)| p != q);
    differ.fold((0, 0), |(count, max), (p, q)| {
        let most = p.iter().zip(q).map(|(x, y)| x.abs_diff(*y)).max();
        (count + 1, max.max(most.unwrap_or(0)))
    })
}

/// Pixels a frame of the cube does not share with the clear color.
fn covered(pixels: &[u8]) -> usize {
    let clear = &pixels[..4];
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|&p| p != clear)
        .count()
}

#[test]
#[ignore = "needs a GPU; scripts/smoke.ps1 runs it"]
fn skinned_cube_on_an_identity_joint_draws_pixel_for_pixel_like_the_built_in_one() {
    let built_in = built_in_cube(cube_model());
    assert!(covered(&built_in) > 200, "{} pixels", covered(&built_in));
    let skinned = skinned_cube(
        cube_model(),
        [0; 4],
        [1.0, 0.0, 0.0, 0.0],
        &[Mat4::IDENTITY],
    );
    assert_eq!(difference(&built_in, &skinned), (0, 0));
}

#[test]
#[ignore = "needs a GPU; scripts/smoke.ps1 runs it"]
fn joint_translation_moves_a_skinned_cube_like_the_draw_models_translation() {
    let step = Mat4::from_translation(Vec3::X);
    let built_in = built_in_cube(cube_model() * step);
    let skinned = skinned_cube(cube_model(), [0; 4], [1.0, 0.0, 0.0, 0.0], &[step]);
    assert!(covered(&built_in) > 200, "{} pixels", covered(&built_in));
    assert_eq!(difference(&built_in, &skinned), (0, 0));
    // And it moved: not where the unmoved cube is.
    assert!(difference(&built_in, &built_in_cube(cube_model())).0 > 100);
}

#[test]
#[ignore = "needs a GPU; scripts/smoke.ps1 runs it"]
fn half_weights_on_opposite_joint_moves_cancel_out() {
    let built_in = built_in_cube(cube_model());
    let skinned = skinned_cube(
        cube_model(),
        [0, 1, 0, 0],
        [0.5, 0.5, 0.0, 0.0],
        &[
            Mat4::from_translation(Vec3::X),
            Mat4::from_translation(Vec3::NEG_X),
        ],
    );
    assert_eq!(difference(&built_in, &skinned), (0, 0));
}

#[test]
#[ignore = "needs a GPU; scripts/smoke.ps1 runs it"]
fn joint_ranges_are_required_on_skinned_meshes_only_and_must_cover_their_joints() {
    let mut r = Renderer::new_headless(WIDTH, HEIGHT).expect("a GPU");
    r.resize(WIDTH, HEIGHT).expect("a small size");
    let v = SkinnedVertex {
        position: [0.0; 3],
        normal: [0.0, 0.0, 1.0],
        color: [1.0; 3],
        joints: [0, 2, 0, 0],
        weights: [0.5, 0.5, 0.0, 0.0],
    };
    let skinned = r
        .create_skinned_mesh(&[v; 3], &[0, 1, 2])
        .expect("a valid mesh");
    let (vertices, indices) = mesh::mesh(MeshKind::Cube);
    let indices: Vec<u32> = indices.into_iter().map(u32::from).collect();
    let plain = r.create_mesh(&vertices, &indices).expect("a valid mesh");
    let message = |e: RenderError| e.to_string();
    assert_eq!(
        r.create_skinned_mesh(&[v; 3], &[0, 1, 3]).map_err(message),
        Err("mesh index 3 is out of range: the mesh has 3 vertices".to_owned())
    );
    let joints = [Mat4::IDENTITY; 4];
    let item = |mesh: DrawMesh, joints: Option<(u32, u32)>| DrawItem {
        mesh,
        model: Mat4::IDENTITY,
        color: Vec3::ONE,
        joints: joints.map(|(first, count)| JointRange { first, count }),
    };
    let mut render = |item: DrawItem| {
        let draws = [item];
        r.render(&Frame {
            view: Mat4::IDENTITY,
            projection: Projection::Perspective { fov_y: 1.0 },
            near: 0.1,
            far: 10.0,
            ambient: Vec3::ONE,
            light: None,
            draws: &draws,
            joints: &joints,
            sprites: &[],
            hud: &[],
            lines: &[],
        })
        .map(|_| ())
        .map_err(message)
    };
    assert_eq!(
        render(item(DrawMesh::Id(skinned), None)),
        Err("draw item uses skinned mesh 0 without a joint range".to_owned())
    );
    let static_error =
        Err("draw item gives a joint range for a mesh that is not skinned".to_owned());
    assert_eq!(
        render(item(DrawMesh::Id(plain), Some((0, 1)))),
        static_error
    );
    assert_eq!(
        render(item(MeshKind::Cube.into(), Some((0, 1)))),
        static_error
    );
    assert_eq!(
        render(item(DrawMesh::Id(skinned), Some((1, 2)))),
        Err("skinned mesh 0 uses joint 2, but its draw item's range has 2 joints".to_owned())
    );
    assert_eq!(
        render(item(DrawMesh::Id(skinned), Some((2, 3)))),
        Err("joint range 2..5 is outside the frame's 4 joints".to_owned())
    );
    assert_eq!(render(item(DrawMesh::Id(skinned), Some((1, 3)))), Ok(()));
    r.shutdown();
}
