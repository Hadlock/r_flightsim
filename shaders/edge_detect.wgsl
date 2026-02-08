@group(0) @binding(0) var depth_tex: texture_depth_2d;
@group(0) @binding(1) var normal_tex: texture_2d<f32>;
@group(0) @binding(2) var object_id_tex: texture_2d<u32>;

const FSBLUE = vec4<f32>(0.10, 0.20, 0.30, 1.0);
const WHITE = vec4<f32>(1.0, 1.0, 1.0, 1.0);

const NORMAL_THRESHOLD: f32 = 0.1;
const DEPTH_THRESHOLD: f32 = 0.05;
const DEPTH_ARTIFACT_CORRECTION: f32 = 3.0;

const NEAR: f32 = 1.0;
const FAR: f32 = 40000.0;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(vertex_index / 2u) * 4.0 - 1.0;
    let y = f32(vertex_index % 2u) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    return out;
}

// Linearize depth from [0,1] NDC to view-space distance
fn linearize_depth(d: f32) -> f32 {
    return NEAR * FAR / (FAR - d * (FAR - NEAR));
}

fn sample_normal(coord: vec2<i32>) -> vec3<f32> {
    return textureLoad(normal_tex, coord, 0).xyz;
}

// Godot-style normal edge: Sobel on scalar difference magnitudes
// Computes length(center - neighbor) for each of the 8 neighbors,
// then applies Sobel kernel to those scalars.
fn edge_normal(coord: vec2<i32>, center: vec3<f32>) -> f32 {
    let nw = length(center - sample_normal(coord + vec2<i32>(-1, -1)));
    let n  = length(center - sample_normal(coord + vec2<i32>( 0, -1)));
    let ne = length(center - sample_normal(coord + vec2<i32>( 1, -1)));
    let w  = length(center - sample_normal(coord + vec2<i32>(-1,  0)));
    let e  = length(center - sample_normal(coord + vec2<i32>( 1,  0)));
    let sw = length(center - sample_normal(coord + vec2<i32>(-1,  1)));
    let s  = length(center - sample_normal(coord + vec2<i32>( 0,  1)));
    let se = length(center - sample_normal(coord + vec2<i32>( 1,  1)));

    // Sobel X: vertical edge detector
    let sx = abs(
        1.0 * nw + 2.0 * n + 1.0 * ne
      + 0.0 * w  + 0.0     + 0.0 * e
      - 1.0 * sw - 2.0 * s - 1.0 * se
    );

    // Sobel Y: horizontal edge detector
    let sy = abs(
        1.0 * nw + 0.0 * n - 1.0 * ne
      + 2.0 * w  + 0.0     - 2.0 * e
      + 1.0 * sw + 0.0 * s - 1.0 * se
    );

    return sx + sy;
}

// Godot-style depth edge: Sobel on relative depth differences
fn edge_depth(coord: vec2<i32>, center: f32) -> f32 {
    let nw = (center - linearize_depth(textureLoad(depth_tex, coord + vec2<i32>(-1, -1), 0))) / center;
    let n  = (center - linearize_depth(textureLoad(depth_tex, coord + vec2<i32>( 0, -1), 0))) / center;
    let ne = (center - linearize_depth(textureLoad(depth_tex, coord + vec2<i32>( 1, -1), 0))) / center;
    let w  = (center - linearize_depth(textureLoad(depth_tex, coord + vec2<i32>(-1,  0), 0))) / center;
    let e  = (center - linearize_depth(textureLoad(depth_tex, coord + vec2<i32>( 1,  0), 0))) / center;
    let sw = (center - linearize_depth(textureLoad(depth_tex, coord + vec2<i32>(-1,  1), 0))) / center;
    let s  = (center - linearize_depth(textureLoad(depth_tex, coord + vec2<i32>( 0,  1), 0))) / center;
    let se = (center - linearize_depth(textureLoad(depth_tex, coord + vec2<i32>( 1,  1), 0))) / center;

    let sx = abs(
        1.0 * nw + 2.0 * n + 1.0 * ne
      + 0.0 * w  + 0.0     + 0.0 * e
      - 1.0 * sw - 2.0 * s - 1.0 * se
    );

    let sy = abs(
        1.0 * nw + 0.0 * n - 1.0 * ne
      + 2.0 * w  + 0.0     - 2.0 * e
      + 1.0 * sw + 0.0 * s - 1.0 * se
    );

    return sx + sy;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_size = textureDimensions(depth_tex);
    let coord = vec2<i32>(in.position.xy);

    // Bounds check — avoid sampling outside texture
    if coord.x <= 0 || coord.y <= 0 || coord.x >= i32(tex_size.x) - 1 || coord.y >= i32(tex_size.y) - 1 {
        return FSBLUE;
    }

    // Skip background pixels (depth at far plane)
    let raw_depth = textureLoad(depth_tex, coord, 0);
    if raw_depth >= 1.0 {
        return FSBLUE;
    }

    let center_depth = linearize_depth(raw_depth);
    let center_normal = sample_normal(coord);

    // --- Normal edge ---
    let n_edge = edge_normal(coord, center_normal);
    if n_edge > NORMAL_THRESHOLD {
        return WHITE;
    }

    // --- Depth edge with artifact correction ---
    // Surfaces at grazing angles get a higher depth threshold
    // to suppress false edges (same trick as the Godot shader)
    let decoded_normal = normalize(center_normal - vec3<f32>(0.5));
    let angle = 1.0 - dot(decoded_normal, vec3<f32>(0.0, 0.0, 1.0));
    let adjusted_depth_threshold = DEPTH_THRESHOLD + angle * DEPTH_ARTIFACT_CORRECTION;

    let d_edge = edge_depth(coord, center_depth);
    if d_edge > adjusted_depth_threshold {
        return WHITE;
    }

    // --- Object ID boundary ---
    let id_c = textureLoad(object_id_tex, coord, 0).r;
    let id_l = textureLoad(object_id_tex, coord + vec2<i32>(-1, 0), 0).r;
    let id_r = textureLoad(object_id_tex, coord + vec2<i32>( 1, 0), 0).r;
    let id_t = textureLoad(object_id_tex, coord + vec2<i32>( 0,-1), 0).r;
    let id_b = textureLoad(object_id_tex, coord + vec2<i32>( 0, 1), 0).r;
    if id_c != id_l || id_c != id_r || id_c != id_t || id_c != id_b {
        return WHITE;
    }

    return FSBLUE;
}
