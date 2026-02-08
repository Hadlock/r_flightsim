struct Uniforms {
    mvp: mat4x4<f32>,
    model_view: mat4x4<f32>,
    object_id: u32,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) view_normal: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = uniforms.mvp * vec4<f32>(in.position, 1.0);
    // Transform normal to view space (using upper-left 3x3 of model_view)
    let normal_mat = mat3x3<f32>(
        uniforms.model_view[0].xyz,
        uniforms.model_view[1].xyz,
        uniforms.model_view[2].xyz,
    );
    out.view_normal = normalize(normal_mat * in.normal);
    return out;
}

struct FragmentOutput {
    @location(0) normal: vec4<f32>,
    @location(1) object_id: u32,
};

@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    out.normal = vec4<f32>(in.view_normal * 0.5 + 0.5, 1.0);
    out.object_id = uniforms.object_id;
    return out;
}
