//! GPU tests of the HUD pipeline: they render a frame and read its pixels
//! back, so they need a GPU and are ignored by `cargo test` (no GPU in
//! `verify.ps1`, `docs/architecture.md` section 2); `scripts/smoke.ps1`
//! runs them with `--ignored`. The frame has nothing but the HUD, over the
//! clear color.

use glam::{Mat4, Vec2, Vec3, Vec4};

use super::*;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 32;

/// The RGBA8 pixels of a frame with only `hud`.
fn render(hud: &[HudDraw]) -> Vec<u8> {
    let mut renderer = Renderer::new_headless(WIDTH, HEIGHT).expect("a GPU");
    renderer.resize(WIDTH, HEIGHT).expect("a small size");
    let drawn = renderer
        .render(&Frame {
            view: Mat4::IDENTITY,
            projection: Projection::Perspective { fov_y: 1.0 },
            near: 0.1,
            far: 10.0,
            ambient: Vec3::ZERO,
            light: None,
            draws: &[],
            sprites: &[],
            hud,
            lines: &[],
        })
        .expect("rendering works");
    assert!(drawn);
    let pixels = renderer.read_pixels().expect("readback works");
    renderer.shutdown();
    pixels
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * WIDTH + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
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

fn clear() -> [u8; 4] {
    [
        srgb(CLEAR_COLOR.r),
        srgb(CLEAR_COLOR.g),
        srgb(CLEAR_COLOR.b),
        255,
    ]
}

fn near(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(&x, y)| x.abs_diff(y) <= 1)
}

#[test]
#[ignore = "needs a GPU; scripts/smoke.ps1 runs it"]
fn hud_rect_covers_exactly_the_pixels_whose_centers_are_inside() {
    let pixels = render(&[HudDraw::Rect {
        min: Vec2::new(8.0, 4.0),
        size: Vec2::new(16.0, 8.0),
        color: Vec4::new(1.0, 0.0, 0.0, 1.0),
    }]);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let inside = (8..24).contains(&x) && (4..12).contains(&y);
            let want = if inside { [255, 0, 0, 255] } else { clear() };
            assert!(near(pixel(&pixels, x, y), want), "({x}, {y})");
        }
    }
}

#[test]
#[ignore = "needs a GPU; scripts/smoke.ps1 runs it"]
fn half_transparent_hud_rect_blends_over_what_is_below_in_linear_space() {
    let pixels = render(&[
        HudDraw::Rect {
            min: Vec2::ZERO,
            size: Vec2::new(64.0, 32.0),
            color: Vec4::new(0.0, 0.0, 1.0, 0.5),
        },
        // Over the first one: later items are drawn over earlier ones.
        HudDraw::Rect {
            min: Vec2::new(32.0, 0.0),
            size: Vec2::new(32.0, 32.0),
            color: Vec4::new(0.0, 1.0, 0.0, 0.25),
        },
    ]);
    let (r, g, b) = (CLEAR_COLOR.r, CLEAR_COLOR.g, CLEAR_COLOR.b);
    let blue = [0.5 * r, 0.5 * g, 0.5 * b + 0.5];
    let want = [srgb(blue[0]), srgb(blue[1]), srgb(blue[2]), 255];
    assert!(
        near(pixel(&pixels, 10, 10), want),
        "{:?}",
        pixel(&pixels, 10, 10)
    );
    let green = [0.75 * blue[0], 0.75 * blue[1] + 0.25, 0.75 * blue[2]];
    let want = [srgb(green[0]), srgb(green[1]), srgb(green[2]), 255];
    assert!(
        near(pixel(&pixels, 40, 10), want),
        "{:?}",
        pixel(&pixels, 40, 10)
    );
}

#[test]
#[ignore = "needs a GPU; scripts/smoke.ps1 runs it"]
fn hud_disc_covers_the_pixels_within_its_radius_with_a_hard_edge() {
    let pixels = render(&[HudDraw::Disc {
        center: Vec2::new(32.0, 16.0),
        radius: 6.0,
        color: Vec4::ONE,
    }]);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            // Pixel centers are at (x + 0.5, y + 0.5); none is within 0.1
            // of the edge here.
            let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(Vec2::new(32.0, 16.0));
            let want = if d <= 6.0 { [255; 4] } else { clear() };
            assert!(near(pixel(&pixels, x, y), want), "({x}, {y}) at {d}");
        }
    }
}

#[test]
#[ignore = "needs a GPU; scripts/smoke.ps1 runs it"]
fn empty_hud_leaves_the_frame_as_cleared() {
    let pixels = render(&[]);
    assert!(pixels.as_chunks::<4>().0.iter().all(|&p| p == clear()));
}
