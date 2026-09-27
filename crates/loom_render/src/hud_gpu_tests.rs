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
    render_with(|_| hud.to_vec())
}

/// The RGBA8 pixels of a frame with only the HUD `make` gives, with the
/// renderer to make textures with.
fn render_with(make: impl FnOnce(&mut Renderer) -> Vec<HudDraw>) -> Vec<u8> {
    let mut renderer = Renderer::new_headless(WIDTH, HEIGHT).expect("a GPU");
    renderer.resize(WIDTH, HEIGHT).expect("a small size");
    let hud = &make(&mut renderer);
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

/// A texture of `pixels` RGBA8 texels, `width` wide.
fn texture(renderer: &mut Renderer, width: u32, pixels: &[[u8; 4]]) -> TextureId {
    let height = pixels.len() as u32 / width;
    renderer
        .create_texture(width, height, pixels.as_flattened())
        .expect("a small texture")
}

/// The glyph 'A' as the HUD font draws it: 5x7, '#' lit.
const GLYPH_A: [&str; 7] = [
    ".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#",
];

const WHITE: [u8; 4] = [255; 4];

/// A 12x8 sheet of two 6x8 cells, the font's layout: a solid white cell,
/// then 'A' at the second cell's top left, at `[6, 0, 5, 7]`.
fn glyph_a_sheet() -> Vec<[u8; 4]> {
    let mut texels = vec![[0; 4]; 12 * 8];
    for y in 0..8 {
        for x in 0..6 {
            texels[y * 12 + x] = WHITE;
        }
        for (x, c) in GLYPH_A.get(y).map_or("", |r| r).chars().enumerate() {
            if c == '#' {
                texels[y * 12 + 6 + x] = WHITE;
            }
        }
    }
    texels
}

/// Whether pixel `(x, y)` is lit by 'A' scaled twice with its top-left
/// corner at `(x0, y0)`.
fn a_lights(x0: u32, y0: u32, x: u32, y: u32) -> bool {
    (x0..x0 + 10).contains(&x)
        && (y0..y0 + 14).contains(&y)
        && GLYPH_A[(y - y0) as usize / 2].as_bytes()[(x - x0) as usize / 2] == b'#'
}

#[test]
#[ignore = "needs a GPU; scripts/smoke.ps1 runs it"]
fn glyph_scaled_twice_lights_exactly_its_pixels_blocks() {
    // Nothing of the sheet's first cell or of the second's blank column
    // and row may show.
    let texels = glyph_a_sheet();
    let pixels = render_with(|r| {
        let texture = texture(r, 12, &texels);
        vec![HudDraw::Image {
            texture,
            min: Vec2::new(10.0, 4.0),
            size: Vec2::new(10.0, 14.0),
            rect: [6, 0, 5, 7],
            color: Vec4::ONE,
        }]
    });
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let want = if a_lights(10, 4, x, y) {
                WHITE
            } else {
                clear()
            };
            assert!(near(pixel(&pixels, x, y), want), "({x}, {y})");
        }
    }
}

#[test]
#[ignore = "needs a GPU; scripts/smoke.ps1 runs it"]
fn glyph_shadow_shows_only_where_the_glyph_does_not_cover_it() {
    // What the engine draws for 'A' at scale 2 with a shadow: the shadow
    // 2 pixels right and down, then the glyph.
    let texels = glyph_a_sheet();
    let body = Vec4::new(1.0, 0.8, 0.2, 1.0);
    let pixels = render_with(|r| {
        let texture = texture(r, 12, &texels);
        let glyph = |at: Vec2, color| HudDraw::Image {
            texture,
            min: at,
            size: Vec2::new(10.0, 14.0),
            rect: [6, 0, 5, 7],
            color,
        };
        vec![
            glyph(Vec2::new(12.0, 6.0), Vec4::new(0.0, 0.0, 0.0, 0.75)),
            glyph(Vec2::new(10.0, 4.0), body),
        ]
    });
    let body = [srgb(1.0), srgb(0.8), srgb(0.2), 255];
    // A quarter of the background shows through the shadow.
    let shadow = [
        srgb(0.25 * CLEAR_COLOR.r),
        srgb(0.25 * CLEAR_COLOR.g),
        srgb(0.25 * CLEAR_COLOR.b),
        255,
    ];
    let (mut bodies, mut shadows) = (0, 0);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let want = if a_lights(10, 4, x, y) {
                bodies += 1;
                body
            } else if a_lights(12, 6, x, y) {
                shadows += 1;
                shadow
            } else {
                clear()
            };
            let got = pixel(&pixels, x, y);
            assert!(near(got, want), "({x}, {y}): {got:?}, want {want:?}");
        }
    }
    // 'A' has 18 lit glyph pixels: 72 screen pixels; its shadow shows on
    // some of its own 72.
    assert_eq!(bodies, 72);
    assert!(shadows > 0 && shadows < 72, "{shadows}");
}

#[test]
#[ignore = "needs a GPU; scripts/smoke.ps1 runs it"]
fn image_texels_are_multiplied_by_its_color_and_transparent_ones_dropped() {
    // White, a color, and transparent: each drawn 8 wide.
    let texels = [[255, 255, 255, 255], [128, 64, 255, 255], [255, 0, 0, 0]];
    let color = Vec4::new(1.0, 0.5, 0.25, 1.0);
    let pixels = render_with(|r| {
        let texture = texture(r, 3, &texels);
        vec![HudDraw::Image {
            texture,
            min: Vec2::new(8.0, 8.0),
            size: Vec2::new(24.0, 8.0),
            rect: [0, 0, 3, 1],
            color,
        }]
    });
    // The texture is sRGB: the shader multiplies the linear values.
    let linear = |s: u8| {
        let s = f64::from(s) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    let white = [srgb(1.0), srgb(0.5), srgb(0.25), 255];
    let tinted = [srgb(linear(128)), srgb(linear(64) * 0.5), srgb(0.25), 255];
    for (x, want) in [(10, white), (18, tinted), (26, clear()), (4, clear())] {
        let got = pixel(&pixels, x, 12);
        assert!(near(got, want), "({x}, 12): {got:?}, want {want:?}");
    }
}

#[test]
#[ignore = "needs a GPU; scripts/smoke.ps1 runs it"]
fn interleaved_solid_and_image_items_cover_each_other_in_list_order() {
    let red = Vec4::new(1.0, 0.0, 0.0, 1.0);
    let blue = Vec4::new(0.0, 0.0, 1.0, 1.0);
    let pixels = render_with(|r| {
        let white = texture(r, 1, &[[255; 4]]);
        let image = |x: f32, color| HudDraw::Image {
            texture: white,
            min: Vec2::new(x, 0.0),
            size: Vec2::new(24.0, 32.0),
            rect: [0, 0, 1, 1],
            color,
        };
        vec![
            // Red everywhere, a green image over x 8..32, a blue rect over
            // x 16..40, a yellow image over x 32..56, a red disc on top.
            HudDraw::Rect {
                min: Vec2::ZERO,
                size: Vec2::new(64.0, 32.0),
                color: red,
            },
            image(8.0, Vec4::new(0.0, 1.0, 0.0, 1.0)),
            HudDraw::Rect {
                min: Vec2::new(16.0, 0.0),
                size: Vec2::new(24.0, 32.0),
                color: blue,
            },
            image(32.0, Vec4::new(1.0, 1.0, 0.0, 1.0)),
            HudDraw::Disc {
                center: Vec2::new(44.0, 16.0),
                radius: 3.0,
                color: red,
            },
        ]
    });
    for (x, want) in [
        (4, [255, 0, 0, 255]),
        (12, [0, 255, 0, 255]),
        (20, [0, 0, 255, 255]),
        (36, [255, 255, 0, 255]),
        (60, [255, 0, 0, 255]),
    ] {
        let got = pixel(&pixels, x, 4);
        assert!(near(got, want), "({x}, 4): {got:?}");
    }
    assert!(near(pixel(&pixels, 44, 16), [255, 0, 0, 255]));
}
