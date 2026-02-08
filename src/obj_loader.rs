use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use std::path::Path;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

pub fn load_obj(path: &Path) -> MeshData {
    let (models, _) = tobj::load_obj(
        path,
        &tobj::LoadOptions {
            triangulate: true,
            single_index: true,
            ..Default::default()
        },
    )
    .expect("Failed to load OBJ file");

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for model in &models {
        let mesh = &model.mesh;
        let base = vertices.len() as u32;

        for i in 0..mesh.positions.len() / 3 {
            vertices.push(Vertex {
                position: [
                    mesh.positions[i * 3],
                    mesh.positions[i * 3 + 1],
                    mesh.positions[i * 3 + 2],
                ],
                normal: [0.0, 0.0, 0.0],
            });
        }

        for &idx in &mesh.indices {
            indices.push(base + idx);
        }
    }

    // Always compute smooth normals by position so the Sobel edge
    // detector only fires on genuine creases, not per-face boundaries.
    compute_smooth_normals(&mut vertices, &indices);

    MeshData { vertices, indices }
}

/// Quantize a float position to an integer key for hashing.
/// Positions within ~0.0001 of each other will share the same key.
fn pos_key(p: [f32; 3]) -> [i32; 3] {
    [
        (p[0] * 10000.0).round() as i32,
        (p[1] * 10000.0).round() as i32,
        (p[2] * 10000.0).round() as i32,
    ]
}

fn compute_smooth_normals(vertices: &mut [Vertex], indices: &[u32]) {
    // Accumulate face normals per unique position (not per vertex index).
    // This handles single_index meshes where the same geometric point
    // appears as multiple vertex entries with different indices.
    let mut pos_normals: HashMap<[i32; 3], [f32; 3]> = HashMap::new();

    for tri in indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let p0 = glam::Vec3::from(vertices[tri[0] as usize].position);
        let p1 = glam::Vec3::from(vertices[tri[1] as usize].position);
        let p2 = glam::Vec3::from(vertices[tri[2] as usize].position);

        let face_normal = (p1 - p0).cross(p2 - p0);

        for &idx in tri {
            let key = pos_key(vertices[idx as usize].position);
            let entry = pos_normals.entry(key).or_insert([0.0; 3]);
            entry[0] += face_normal.x;
            entry[1] += face_normal.y;
            entry[2] += face_normal.z;
        }
    }

    // Write back normalized smooth normals
    for v in vertices.iter_mut() {
        let key = pos_key(v.position);
        if let Some(acc) = pos_normals.get(&key) {
            let n = glam::Vec3::from(*acc);
            let len = n.length();
            if len > 0.0 {
                v.normal = (n / len).into();
            } else {
                v.normal = [0.0, 1.0, 0.0];
            }
        }
    }
}
