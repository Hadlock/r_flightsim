use glam::{DVec3, Quat};
use wgpu::util::DeviceExt;

use crate::coords::{self, LLA};
use crate::obj_loader::Vertex;
use crate::scene::SceneObject;

/// LOD altitude thresholds: (max_altitude_m, lat/lon_step_degrees)
const LOD_LEVELS: [(f64, f64); 5] = [
    (1_524.0, 2.0),    //     0 – 5,000 ft:   2° grid
    (9_144.0, 4.0),    // 5,000 – 30,000 ft:  4° grid
    (30_480.0, 6.0),   // 30,000 – 100,000 ft: 6° grid
    (500_000.0, 10.0), // 100,000 ft – 500 km: 10° grid
    (f64::MAX, 15.0),  // 500 km+:             15° grid
];

/// Rebuild threshold distances per LOD (meters camera must move)
const REBUILD_THRESHOLD: [f64; 5] = [100.0, 100.0, 10_000.0, 10_000.0, 10_000.0];

struct EarthLodData {
    vertices_ecef: Vec<DVec3>,
    grid_indices: Vec<u32>,
    tri_count: usize,
}

pub struct EarthRenderer {
    lods: Vec<EarthLodData>,
    current_lod: usize,
    last_camera_ecef: DVec3,
    last_rebuild_lod: usize,
}

impl EarthRenderer {
    /// Create the EarthRenderer and an initial SceneObject to insert into the objects list.
    pub fn new(device: &wgpu::Device) -> (Self, SceneObject) {
        let lods: Vec<EarthLodData> = LOD_LEVELS
            .iter()
            .map(|&(_, step)| generate_lod(step))
            .collect();

        log::info!(
            "[earth] Generated {} LODs: {}",
            lods.len(),
            lods.iter()
                .enumerate()
                .map(|(i, l)| format!(
                    "LOD{} {}° {}k tris",
                    i,
                    LOD_LEVELS[i].1,
                    l.tri_count / 1000
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );

        // Build initial buffers with LOD 0
        let lod = &lods[0];
        let vertices = build_gpu_vertices(lod, DVec3::ZERO);
        let scene_obj = create_earth_scene_object(device, &vertices, DVec3::ZERO);

        let renderer = Self {
            lods,
            current_lod: 0,
            last_camera_ecef: DVec3::new(f64::MAX, 0.0, 0.0),
            last_rebuild_lod: usize::MAX,
        };

        (renderer, scene_obj)
    }

    /// Update the earth mesh. Rebuilds the SceneObject's GPU buffers when needed.
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        scene_obj: &mut SceneObject,
        camera_pos_ecef: DVec3,
        altitude_m: f64,
    ) {
        let new_lod = select_lod(altitude_m);
        let camera_moved = (camera_pos_ecef - self.last_camera_ecef).length();
        let threshold = REBUILD_THRESHOLD[new_lod];

        let needs_rebuild =
            new_lod != self.last_rebuild_lod || camera_moved > threshold;

        if !needs_rebuild {
            // Still update world_pos so model_matrix_relative_to zeroes out
            scene_obj.world_pos = camera_pos_ecef;
            return;
        }

        let lod = &self.lods[new_lod];
        let vertices = build_gpu_vertices(lod, camera_pos_ecef);
        let vertex_count = vertices.len() as u32;

        scene_obj.vertex_buf =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Earth Vertex Buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        if new_lod != self.current_lod {
            let seq_indices: Vec<u32> = (0..vertex_count).collect();
            scene_obj.index_buf =
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Earth Index Buffer"),
                    contents: bytemuck::cast_slice(&seq_indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
            self.current_lod = new_lod;
        }

        scene_obj.index_count = vertex_count;
        scene_obj.world_pos = camera_pos_ecef;
        self.last_camera_ecef = camera_pos_ecef;
        self.last_rebuild_lod = new_lod;
    }
}

fn select_lod(altitude_m: f64) -> usize {
    for (i, &(max_alt, _)) in LOD_LEVELS.iter().enumerate() {
        if altitude_m <= max_alt {
            return i;
        }
    }
    LOD_LEVELS.len() - 1
}

fn generate_lod(step_deg: f64) -> EarthLodData {
    let lat_steps = (180.0 / step_deg).round() as i32;
    let lon_steps = (360.0 / step_deg).round() as i32;

    let mut vertices_ecef = Vec::new();

    for i in 0..=lat_steps {
        let lat = -90.0 + (i as f64) * step_deg;
        let lat_r = lat.to_radians();
        for j in 0..=lon_steps {
            let lon = -180.0 + (j as f64) * step_deg;
            let lon_r = lon.to_radians();
            vertices_ecef.push(coords::lla_to_ecef(&LLA {
                lat: lat_r,
                lon: lon_r,
                alt: 0.0,
            }));
        }
    }

    let cols = (lon_steps + 1) as u32;
    let mut grid_indices = Vec::new();
    for i in 0..lat_steps as u32 {
        for j in 0..lon_steps as u32 {
            let tl = i * cols + j;
            let tr = i * cols + j + 1;
            let bl = (i + 1) * cols + j;
            let br = (i + 1) * cols + j + 1;
            grid_indices.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
        }
    }

    let tri_count = grid_indices.len() / 3;
    EarthLodData {
        vertices_ecef,
        grid_indices,
        tri_count,
    }
}

/// Build GPU vertices with camera-relative positions and flat face normals.
fn build_gpu_vertices(lod: &EarthLodData, camera_pos: DVec3) -> Vec<Vertex> {
    let rel_positions: Vec<[f32; 3]> = lod
        .vertices_ecef
        .iter()
        .map(|pos| {
            let rel = *pos - camera_pos;
            [rel.x as f32, rel.y as f32, rel.z as f32]
        })
        .collect();

    let mut vertices = Vec::with_capacity(lod.tri_count * 3);

    for tri in 0..lod.tri_count {
        let i0 = lod.grid_indices[tri * 3] as usize;
        let i1 = lod.grid_indices[tri * 3 + 1] as usize;
        let i2 = lod.grid_indices[tri * 3 + 2] as usize;

        let p0 = rel_positions[i0];
        let p1 = rel_positions[i1];
        let p2 = rel_positions[i2];

        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let nx = e1[1] * e2[2] - e1[2] * e2[1];
        let ny = e1[2] * e2[0] - e1[0] * e2[2];
        let nz = e1[0] * e2[1] - e1[1] * e2[0];
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        let normal = if len > 1e-10 {
            [nx / len, ny / len, nz / len]
        } else {
            [0.0, 0.0, 1.0]
        };

        vertices.push(Vertex { position: p0, normal });
        vertices.push(Vertex { position: p1, normal });
        vertices.push(Vertex { position: p2, normal });
    }

    vertices
}

fn create_earth_scene_object(
    device: &wgpu::Device,
    vertices: &[Vertex],
    camera_pos: DVec3,
) -> SceneObject {
    let vertex_count = vertices.len() as u32;
    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Earth Vertex Buffer"),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let seq_indices: Vec<u32> = (0..vertex_count).collect();
    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Earth Index Buffer"),
        contents: bytemuck::cast_slice(&seq_indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    SceneObject {
        name: "earth".to_string(),
        vertex_buf,
        index_buf,
        index_count: vertex_count,
        world_pos: camera_pos,
        rotation: Quat::IDENTITY,
        scale: 1.0,
        object_id: 2,
        edges_enabled: true,
    }
}

/// Dynamic far plane based on altitude above ellipsoid.
pub fn dynamic_far_plane(altitude_m: f64) -> f32 {
    if altitude_m < 10_000.0 {
        40_000.0
    } else if altitude_m < 100_000.0 {
        500_000.0
    } else if altitude_m < 1_000_000.0 {
        5_000_000.0
    } else if altitude_m < 50_000_000.0 {
        100_000_000.0
    } else {
        500_000_000.0
    }
}

/// Dynamic near plane based on altitude to preserve depth precision.
pub fn dynamic_near_plane(altitude_m: f64) -> f32 {
    if altitude_m < 10_000.0 {
        1.0
    } else if altitude_m < 1_000_000.0 {
        100.0
    } else {
        10_000.0
    }
}
