// Debug lines: unlit, one color per vertex, world-space positions. The
// pipeline has no depth test, so they draw over everything before them.
// Colors are linear; the sRGB target encodes them.

struct Globals {
    view_proj: mat4x4<f32>,
    ambient: vec4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip = globals.view_proj * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
