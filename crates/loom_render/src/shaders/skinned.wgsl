// Lit skinned meshes: mesh.wgsl with each vertex first placed by up to four
// joint matrices of its draw item's range, blended by its weights (which
// sum to 1). The normal goes through the blended matrix's 3x3 and is
// normalized (the joint matrices are rigid), then through the draw's
// normal matrix as in mesh.wgsl. The lighting is mesh.wgsl's: keep in
// sync with mesh.wgsl.

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
    // x: the first joint matrix of the draw's range.
    joints: vec4<u32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

@group(0) @binding(1)
var<storage, read> draws: array<Draw>;

@group(0) @binding(2)
var<storage, read> joints: array<mat4x4<f32>>;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) joints: vec4<u32>,
    @location(4) weights: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn vs_main(in: VertexIn, @builtin(instance_index) index: u32) -> VertexOut {
    let draw = draws[index];
    let first = draw.joints.x;
    let skin = joints[first + in.joints.x] * in.weights.x
        + joints[first + in.joints.y] * in.weights.y
        + joints[first + in.joints.z] * in.weights.z
        + joints[first + in.joints.w] * in.weights.w;
    let skin3 = mat3x3<f32>(skin[0].xyz, skin[1].xyz, skin[2].xyz);
    let normal_matrix = mat3x3<f32>(draw.normal0.xyz, draw.normal1.xyz, draw.normal2.xyz);
    var out: VertexOut;
    out.clip = globals.view_proj * draw.model * (skin * vec4<f32>(in.position, 1.0));
    out.normal = normal_matrix * normalize(skin3 * in.normal);
    out.color = draw.color.rgb * in.color;
    return out;
}

// Keep in sync with mesh.wgsl.
@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let diffuse = max(0.0, dot(n, -globals.light_dir.xyz));
    let light = globals.ambient.rgb + diffuse * globals.light_color.rgb;
    return vec4<f32>(in.color * light, 1.0);
}
