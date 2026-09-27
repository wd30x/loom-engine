// HUD: screen-space quads already in NDC, drawn in order with alpha
// blending and no depth test. Colors are linear; blending happens in linear
// space and the sRGB target encodes the result.
//
// `fs_solid` draws rectangles and discs: `local` runs from -1 to 1 across a
// disc's bounding square (0 everywhere on a rectangle), and fragments
// outside the unit circle are dropped, a hard edge.
//
// `fs_textured` draws text and images: the texel of `rect` under `uv` (0 at
// the quad's top-left corner, 1 at the bottom-right), fetched with
// textureLoad like a sprite's; alpha < 0.5 is dropped, the rest is the
// texel's color times `color` with `color`'s alpha.

struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) local: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) rect: vec4<u32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) rect: vec4<u32>,
};

@group(0) @binding(0)
var sheet: texture_2d<f32>;

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip = vec4<f32>(in.position, 0.0, 1.0);
    out.local = in.local;
    out.color = in.color;
    out.uv = in.uv;
    out.rect = in.rect;
    return out;
}

@fragment
fn fs_solid(in: VertexOut) -> @location(0) vec4<f32> {
    if (dot(in.local, in.local) > 1.0) {
        discard;
    }
    return in.color;
}

@fragment
fn fs_textured(in: VertexOut) -> @location(0) vec4<f32> {
    let size = in.rect.zw;
    // Converting a (tiny) negative float to u32 saturates to 0.
    let offset = min(vec2<u32>(floor(in.uv * vec2<f32>(size))), size - vec2<u32>(1u));
    let texel = textureLoad(sheet, in.rect.xy + offset, 0);
    if (texel.a < 0.5) {
        discard;
    }
    return vec4<f32>(texel.rgb * in.color.rgb, in.color.a);
}
