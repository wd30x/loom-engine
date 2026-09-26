// Lit, flat-colored meshes. One storage-buffer entry per draw item, picked by
// instance index. Colors are linear; the sRGB target encodes them.
//   color * (ambient + max(0, dot(n, -light_dir)) * light_color)
// where light_color already includes the intensity (zero without a light).

struct Globals {
    view_proj: mat4x4<f32>,
    ambient: vec4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
};

struct Draw {
    model: mat4x4<f32>,
    normal0: vec4<f32>,
    normal1: vec4<f32>,
    normal2: vec4<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

@group(0) @binding(1)
var<storage, read> draws: array<Draw>;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn vs_main(in: VertexIn, @builtin(instance_index) index: u32) -> VertexOut {
    let draw = draws[index];
    let normal_matrix = mat3x3<f32>(draw.normal0.xyz, draw.normal1.xyz, draw.normal2.xyz);
    var out: VertexOut;
    out.clip = globals.view_proj * draw.model * vec4<f32>(in.position, 1.0);
    out.normal = normal_matrix * in.normal;
    out.color = draw.color.rgb;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let diffuse = max(0.0, dot(n, -globals.light_dir.xyz));
    let light = globals.ambient.rgb + diffuse * globals.light_color.rgb;
    return vec4<f32>(in.color * light, 1.0);
}
