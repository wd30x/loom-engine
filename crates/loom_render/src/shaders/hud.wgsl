// HUD: screen-space rectangles and discs, already in NDC, drawn in order
// with alpha blending and no depth test. `local` runs from -1 to 1 across a
// disc's bounding square (0 everywhere on a rectangle): fragments outside
// the unit circle are dropped, a hard edge. Colors are linear; blending
// happens in linear space and the sRGB target encodes the result.

struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) local: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip = vec4<f32>(in.position, 0.0, 1.0);
    out.local = in.local;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    if (dot(in.local, in.local) > 1.0) {
        discard;
    }
    return in.color;
}
