// Copies the offscreen target to the surface with one fullscreen triangle.
// Both sides are the same size, so texels are fetched 1:1 without a sampler.
// Reading the sRGB source decodes to linear; the sRGB surface view re-encodes.

@group(0) @binding(0)
var source: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // (-1,-1), (3,-1), (-1,3): covers the whole viewport.
    let x = f32((index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(index & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(source, vec2<i32>(frag.xy), 0);
}
