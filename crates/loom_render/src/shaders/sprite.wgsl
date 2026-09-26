// Unlit, alpha-tested sprites. One storage-buffer entry per sprite, picked by
// instance index; each is a 1x1 quad on its model's XY plane, centered at the
// origin. Pixels are fetched with textureLoad (no sampler, so no filtering and
// no bleeding from neighbouring frames); alpha < 0.5 is discarded, so depth
// alone decides what is in front. The sRGB texture decodes to linear and the
// sRGB target re-encodes.

struct Globals {
    view_proj: mat4x4<f32>,
    ambient: vec4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
};

struct Sprite {
    model: mat4x4<f32>,
    // x, y, width, height in texture pixels.
    rect: vec4<u32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

@group(0) @binding(1)
var<storage, read> sprites: array<Sprite>;

@group(1) @binding(0)
var sheet: texture_2d<f32>;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    // (0, 0) at the quad's top-left corner, (1, 1) at the bottom-right.
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) rect: vec4<u32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) index: u32,
) -> VertexOut {
    // Two triangles, counter-clockwise seen from +Z.
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, -0.5),
        vec2<f32>(0.5, -0.5),
        vec2<f32>(0.5, 0.5),
        vec2<f32>(-0.5, -0.5),
        vec2<f32>(0.5, 0.5),
        vec2<f32>(-0.5, 0.5),
    );
    let p = corners[vertex];
    let sprite = sprites[index];
    var out: VertexOut;
    out.clip = globals.view_proj * sprite.model * vec4<f32>(p, 0.0, 1.0);
    out.uv = vec2<f32>(p.x + 0.5, 0.5 - p.y);
    out.rect = sprite.rect;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let size = in.rect.zw;
    // Converting a (tiny) negative float to u32 saturates to 0.
    let offset = min(vec2<u32>(floor(in.uv * vec2<f32>(size))), size - vec2<u32>(1u));
    let color = textureLoad(sheet, in.rect.xy + offset, 0);
    if color.a < 0.5 {
        discard;
    }
    return color;
}
