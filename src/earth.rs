use glam::{DVec3, Quat};
use wgpu::util::DeviceExt;

use crate::coords::{self, LLA};
use crate::obj_loader::Vertex;
use crate::scene::SceneObject;

/// LOD altitude thresholds: (max_altitude_m, lat/lon_step_degrees)
const LOD_LEVELS: [(f64, f64); 7] = [
    (1_524.0, 2.0),        //     0 – 5,000 ft:   2° grid  (~32k tris)
    (9_144.0, 4.0),        // 5,000 – 30,000 ft:  4° grid  (~8k tris)
    (30_480.0, 6.0),       // 30,000 – 100,000 ft: 6° grid (~3.6k tris)
    (500_000.0, 10.0),     // 100,000 ft – 500 km: 10° grid (~1.3k tris)
    (2_000_000.0, 2.0),    // 500 km – 2,000 km:  2° grid  (~32k tris, smooth orbital view)
    (10_000_000.0, 4.0),   // 2,000 – 10,000 km:  4° grid  (~8k tris)
    (f64::MAX, 10.0),      // 10,000 km+:          10° grid (~1.3k tris)
];

/// Rebuild threshold distances per LOD (meters camera must move).
/// Orbital LODs use 0.0 = rebuild every frame (orbital speeds cause visible snapping otherwise).
const REBUILD_THRESHOLD: [f64; 7] = [100.0, 100.0, 10_000.0, 10_000.0, 0.0, 0.0, 0.0];

struct EarthLodData {
    vertices_ecef: Vec<DVec3>,
    /// Geodetic surface normal per vertex (constant, precomputed)
    normals: Vec<[f32; 3]>,
    /// Triangle indices (CCW from outside)
    indices: Vec<u32>,
}

pub struct EarthRenderer {
    lods: Vec<EarthLodData>,
    current_lod: usize,
    last_camera_ecef: DVec3,
    last_rebuild_lod: usize,
    /// Reusable scratch buffer for building camera-relative vertices (avoids per-frame heap alloc)
    vertex_scratch: Vec<Vertex>,
}

impl EarthRenderer {
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
                    l.indices.len() / 3000
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );

        // LOD 0 has the most vertices — use it for max buffer sizing
        let max_vertices = lods.iter().map(|l| l.vertices_ecef.len()).max().unwrap_or(0);

        let lod = &lods[0];
        let vertices = build_gpu_vertices(lod, DVec3::ZERO);
        let scene_obj = create_scene_object(device, &vertices, &lod.indices, DVec3::ZERO);

        let renderer = Self {
            lods,
            current_lod: 0,
            last_camera_ecef: DVec3::new(f64::MAX, 0.0, 0.0),
            last_rebuild_lod: usize::MAX,
            vertex_scratch: Vec::with_capacity(max_vertices),
        };

        (renderer, scene_obj)
    }

    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
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
            scene_obj.world_pos = camera_pos_ecef;
            return;
        }

        let lod = &self.lods[new_lod];

        // Fill scratch buffer with camera-relative vertices (reuses heap allocation)
        self.vertex_scratch.clear();
        self.vertex_scratch.extend(
            lod.vertices_ecef
                .iter()
                .zip(lod.normals.iter())
                .map(|(pos, normal)| {
                    let rel = *pos - camera_pos_ecef;
                    Vertex {
                        position: [rel.x as f32, rel.y as f32, rel.z as f32],
                        normal: *normal,
                    }
                }),
        );

        // Update vertex buffer in-place (avoids GPU memory alloc/free per rebuild)
        queue.write_buffer(
            &scene_obj.vertex_buf,
            0,
            bytemuck::cast_slice(&self.vertex_scratch),
        );

        if new_lod != self.current_lod {
            scene_obj.index_buf =
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Earth Index Buffer"),
                    contents: bytemuck::cast_slice(&lod.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
            scene_obj.index_count = lod.indices.len() as u32;
            self.current_lod = new_lod;
        }

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
    let mut normals = Vec::new();

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
            // Geodetic surface normal = ENU "up" vector
            let (slat, clat) = lat_r.sin_cos();
            let (slon, clon) = lon_r.sin_cos();
            normals.push([
                (clat * clon) as f32,
                (clat * slon) as f32,
                slat as f32,
            ]);
        }
    }

    // Triangle indices — CCW winding from OUTSIDE the earth
    let cols = (lon_steps + 1) as u32;
    let mut indices = Vec::new();
    for i in 0..lat_steps as u32 {
        for j in 0..lon_steps as u32 {
            let tl = i * cols + j;
            let tr = i * cols + j + 1;
            let bl = (i + 1) * cols + j;
            let br = (i + 1) * cols + j + 1;
            // tl=south-west, tr=south-east, bl=north-west, br=north-east
            // For outward-facing CCW: tl → tr → bl, then tr → br → bl
            indices.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
        }
    }

    EarthLodData {
        vertices_ecef,
        normals,
        indices,
    }
}

/// Build GPU vertex buffer with camera-relative positions and smooth geodetic normals.
fn build_gpu_vertices(lod: &EarthLodData, camera_pos: DVec3) -> Vec<Vertex> {
    lod.vertices_ecef
        .iter()
        .zip(lod.normals.iter())
        .map(|(pos, normal)| {
            let rel = *pos - camera_pos;
            Vertex {
                position: [rel.x as f32, rel.y as f32, rel.z as f32],
                normal: *normal,
            }
        })
        .collect()
}

fn create_scene_object(
    device: &wgpu::Device,
    vertices: &[Vertex],
    indices: &[u32],
    camera_pos: DVec3,
) -> SceneObject {
    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Earth Vertex Buffer"),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    });
    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Earth Index Buffer"),
        contents: bytemuck::cast_slice(indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    SceneObject {
        name: "earth".to_string(),
        vertex_buf,
        index_buf,
        index_count: indices.len() as u32,
        world_pos: camera_pos,
        rotation: Quat::IDENTITY,
        scale: 1.0,
        object_id: 2,
        edges_enabled: true,
        bounding_radius: f32::MAX, // never cull earth
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
    } else if altitude_m < 500_000_000.0 {
        500_000_000.0           // 500,000 km — lunar distance
    } else {
        2_000_000_000.0         // 2B m = 2M km — encompasses L1 (1.5M km)
    }
}
