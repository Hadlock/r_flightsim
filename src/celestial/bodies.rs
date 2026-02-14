use crate::obj_loader::{MeshData, Vertex};
use glam::DVec3;

/// Generate an icosphere by recursive subdivision of a regular icosahedron.
/// `subdivisions` = 0 gives the base icosahedron (12 verts, 20 tris).
/// Each subdivision roughly quadruples the triangle count.
pub fn generate_icosphere(subdivisions: u32) -> MeshData {
    let t = (1.0 + 5.0_f32.sqrt()) / 2.0;

    // 12 vertices of a regular icosahedron
    let mut positions: Vec<[f32; 3]> = vec![
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ];

    // Normalize to unit sphere
    for p in &mut positions {
        let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        p[0] /= len;
        p[1] /= len;
        p[2] /= len;
    }

    let mut indices: Vec<u32> = vec![
        0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11,
        1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7, 6, 7, 1, 8,
        3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9,
        4, 9, 5, 2, 4, 11, 6, 2, 10, 8, 6, 7, 9, 8, 1,
    ];

    use std::collections::HashMap;

    for _ in 0..subdivisions {
        let mut new_indices = Vec::with_capacity(indices.len() * 4);
        let mut midpoint_cache: HashMap<(u32, u32), u32> = HashMap::new();

        let mut get_midpoint = |a: u32, b: u32, positions: &mut Vec<[f32; 3]>| -> u32 {
            let key = if a < b { (a, b) } else { (b, a) };
            if let Some(&idx) = midpoint_cache.get(&key) {
                return idx;
            }
            let pa = positions[a as usize];
            let pb = positions[b as usize];
            let mut mid = [
                (pa[0] + pb[0]) / 2.0,
                (pa[1] + pb[1]) / 2.0,
                (pa[2] + pb[2]) / 2.0,
            ];
            let len = (mid[0] * mid[0] + mid[1] * mid[1] + mid[2] * mid[2]).sqrt();
            mid[0] /= len;
            mid[1] /= len;
            mid[2] /= len;
            let idx = positions.len() as u32;
            positions.push(mid);
            midpoint_cache.insert(key, idx);
            idx
        };

        for tri in indices.chunks(3) {
            let a = tri[0];
            let b = tri[1];
            let c = tri[2];
            let ab = get_midpoint(a, b, &mut positions);
            let bc = get_midpoint(b, c, &mut positions);
            let ca = get_midpoint(c, a, &mut positions);
            new_indices.extend_from_slice(&[a, ab, ca]);
            new_indices.extend_from_slice(&[b, bc, ab]);
            new_indices.extend_from_slice(&[c, ca, bc]);
            new_indices.extend_from_slice(&[ab, bc, ca]);
        }

        indices = new_indices;
    }

    let vertices: Vec<Vertex> = positions
        .iter()
        .map(|p| Vertex {
            position: *p,
            normal: *p, // for a unit sphere, normal = position
        })
        .collect();

    MeshData { vertices, indices }
}

/// Generate a unit cube (side length 2, centered at origin).
pub fn generate_unit_cube() -> MeshData {
    let positions: [[f32; 3]; 8] = [
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];

    // Face normals for each face's vertices
    let face_data: [([usize; 4], [f32; 3]); 6] = [
        ([0, 1, 2, 3], [0.0, 0.0, -1.0]), // back
        ([4, 5, 6, 7], [0.0, 0.0, 1.0]),  // front
        ([0, 4, 7, 3], [-1.0, 0.0, 0.0]), // left
        ([1, 5, 6, 2], [1.0, 0.0, 0.0]),  // right
        ([0, 1, 5, 4], [0.0, -1.0, 0.0]), // bottom
        ([3, 2, 6, 7], [0.0, 1.0, 0.0]),  // top
    ];

    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    for (face_idx, normal) in &face_data {
        let base = vertices.len() as u32;
        for &vi in face_idx {
            vertices.push(Vertex {
                position: positions[vi],
                normal: *normal,
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    MeshData { vertices, indices }
}

/// Build a merged mesh of N cube instances at camera-relative positions.
/// Each cube is placed at `camera_ecef + direction * render_distance` and scaled
/// to subtend the given angular size.
pub fn build_merged_cubes(
    cube: &MeshData,
    directions: &[DVec3],
    angular_sizes: &[f64],
    render_distance: f64,
    camera_ecef: DVec3,
) -> MeshData {
    let mut vertices = Vec::with_capacity(directions.len() * cube.vertices.len());
    let mut indices = Vec::with_capacity(directions.len() * cube.indices.len());

    for (dir, &ang_size) in directions.iter().zip(angular_sizes) {
        let pos = camera_ecef + *dir * render_distance;
        let rel = pos - camera_ecef; // = dir * render_distance
        let scale = render_distance * (ang_size / 2.0).tan();

        let base_idx = vertices.len() as u32;
        for v in &cube.vertices {
            vertices.push(Vertex {
                position: [
                    (rel.x as f32) + v.position[0] * scale as f32,
                    (rel.y as f32) + v.position[1] * scale as f32,
                    (rel.z as f32) + v.position[2] * scale as f32,
                ],
                normal: v.normal,
            });
        }
        for idx in &cube.indices {
            indices.push(base_idx + idx);
        }
    }

    MeshData { vertices, indices }
}

/// Build sun icosphere mesh at camera-relative position.
pub fn build_sun_mesh(
    icosphere: &MeshData,
    sun_direction: DVec3,
    render_distance: f64,
    angular_diameter_rad: f64,
) -> MeshData {
    let rel = sun_direction * render_distance;
    let radius = render_distance * (angular_diameter_rad / 2.0).tan();

    let vertices: Vec<Vertex> = icosphere
        .vertices
        .iter()
        .map(|v| Vertex {
            position: [
                (rel.x as f32) + v.position[0] * radius as f32,
                (rel.y as f32) + v.position[1] * radius as f32,
                (rel.z as f32) + v.position[2] * radius as f32,
            ],
            normal: v.normal,
        })
        .collect();

    MeshData {
        vertices,
        indices: icosphere.indices.clone(),
    }
}

/// Build moon mesh. When far (>threshold), uses angular-size trick.
/// When close, uses true position with camera-relative vertex rebuild.
pub fn build_moon_mesh(
    icosphere: &MeshData,
    moon_ecef: DVec3,
    moon_distance_m: f64,
    moon_diameter_m: f64,
    camera_ecef: DVec3,
    render_distance: f64,
    true_render_threshold: f64,
) -> MeshData {
    let to_moon = moon_ecef - camera_ecef;
    let dist = to_moon.length();

    if dist > true_render_threshold {
        // Angular-size trick: render at fixed distance in correct direction
        let dir = to_moon / dist;
        let angular_diameter = 2.0 * ((moon_diameter_m / 2.0) / moon_distance_m).atan();
        let radius = render_distance * (angular_diameter / 2.0).tan();
        let rel = dir * render_distance;

        let vertices: Vec<Vertex> = icosphere
            .vertices
            .iter()
            .map(|v| Vertex {
                position: [
                    (rel.x as f32) + v.position[0] * radius as f32,
                    (rel.y as f32) + v.position[1] * radius as f32,
                    (rel.z as f32) + v.position[2] * radius as f32,
                ],
                normal: v.normal,
            })
            .collect();

        MeshData {
            vertices,
            indices: icosphere.indices.clone(),
        }
    } else {
        // True position: camera-relative vertex rebuild
        let moon_radius = moon_diameter_m / 2.0;

        let vertices: Vec<Vertex> = icosphere
            .vertices
            .iter()
            .map(|v| {
                // Unit sphere vertex → moon surface point in ECEF
                let surface_ecef = moon_ecef
                    + DVec3::new(
                        v.position[0] as f64 * moon_radius,
                        v.position[1] as f64 * moon_radius,
                        v.position[2] as f64 * moon_radius,
                    );
                let rel = surface_ecef - camera_ecef;
                Vertex {
                    position: [rel.x as f32, rel.y as f32, rel.z as f32],
                    normal: v.normal,
                }
            })
            .collect();

        MeshData {
            vertices,
            indices: icosphere.indices.clone(),
        }
    }
}
