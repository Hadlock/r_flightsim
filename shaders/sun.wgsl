// Solid-color overlay shader for the sun.
// Renders the sun as a filled white disc, bypassing Sobel edge detection.
// Uses the same uniform layout as the geometry shader so we can reuse
// the existing uniform buffer and bind group.

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

@vertex
fn vs_main(in: VertexInput) -> @builtin(position) vec4<f32> {
    return uniforms.mvp * vec4<f32>(in.position, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}
