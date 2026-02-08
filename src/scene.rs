use glam::{DVec3, Mat4, Quat};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use wgpu::util::DeviceExt;

use crate::coords::{self, ENUFrame, LLA};
use crate::obj_loader::{self, MeshData};

pub struct SceneObject {
    pub name: String,
    pub vertex_buf: wgpu::Buffer,
    pub index_buf: wgpu::Buffer,
    pub index_count: u32,
    pub world_pos: DVec3,
    pub rotation: Quat,
    pub scale: f32,
    pub object_id: u32,
    pub edges_enabled: bool,
}

impl SceneObject {
    pub fn model_matrix_relative_to(&self, camera_pos: DVec3) -> Mat4 {
        let rel = self.world_pos - camera_pos;
        let translation = Mat4::from_translation(rel.as_vec3());
        let rotation = Mat4::from_quat(self.rotation);
        let scale = Mat4::from_scale(glam::Vec3::splat(self.scale));
        translation * rotation * scale
    }
}

struct MeshBuffers {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
}

fn upload_mesh(device: &wgpu::Device, mesh: &MeshData, label: &str) -> MeshBuffers {
    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("{} Vertex Buffer", label)),
        contents: bytemuck::cast_slice(&mesh.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("{} Index Buffer", label)),
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    MeshBuffers {
        vertex_buf,
        index_buf,
        index_count: mesh.indices.len() as u32,
    }
}

fn spawn(
    device: &wgpu::Device,
    mesh: &MeshData,
    name: &str,
    pos: DVec3,
    rotation: Quat,
    scale: f32,
    object_id: u32,
) -> SceneObject {
    let bufs = upload_mesh(device, mesh, name);
    SceneObject {
        name: name.to_string(),
        vertex_buf: bufs.vertex_buf,
        index_buf: bufs.index_buf,
        index_count: bufs.index_count,
        world_pos: pos,
        rotation,
        scale,
        object_id,
        edges_enabled: true,
    }
}

/// Load the Ki-61 aircraft model as a SceneObject.
/// Position and rotation are set to defaults — caller updates them each frame.
pub fn load_aircraft_object(device: &wgpu::Device, object_id: u32) -> SceneObject {
    let mesh = obj_loader::load_obj(Path::new(
        "assets/14082_WWII_Plane_Japan_Kawasaki_Ki-61_v1_L2.obj",
    ));
    // Ki-61: 12m wingspan, OBJ wingspan extent is ~2.2019 units
    let scale = 12.0 / 2.2019;
    spawn(
        device,
        &mesh,
        "aircraft",
        DVec3::ZERO,
        Quat::IDENTITY,
        scale as f32,
        object_id,
    )
}

// ── Origin tag parser ──────────────────────────────────────────────────

/// Parse an `# origin:` comment from the first 10 lines of an OBJ file.
/// Supports DMS (`37°36'57.5"N 122°23'16.6"W`) and decimal (`37.795200, -122.402800`).
/// Returns lat/lon in radians, alt = 0.
fn parse_origin(path: &Path) -> Option<LLA> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines().take(10).flatten() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("# origin:") {
            let rest = rest.trim();
            if let Some(lla) = parse_dms(rest) {
                return Some(lla);
            }
            if let Some(lla) = parse_decimal(rest) {
                return Some(lla);
            }
        }
    }
    None
}

/// Parse a single DMS component like `37°36'57.5"N` → signed degrees.
fn parse_dms_component(s: &str) -> Option<f64> {
    let s = s.trim();
    let direction = s.chars().last()?;
    if !matches!(direction, 'N' | 'S' | 'E' | 'W') {
        return None;
    }
    let s = &s[..s.len() - direction.len_utf8()];

    let deg_end = s.find('°')?;
    let min_end = s.find('\'')?;
    let sec_end = s.find('"')?;

    let degrees: f64 = s[..deg_end].parse().ok()?;
    let minutes: f64 = s[deg_end + '°'.len_utf8()..min_end].parse().ok()?;
    let seconds: f64 = s[min_end + 1..sec_end].parse().ok()?;

    let mut value = degrees + minutes / 60.0 + seconds / 3600.0;
    if direction == 'S' || direction == 'W' {
        value = -value;
    }
    Some(value)
}

/// Parse DMS format: `37°36'57.5"N 122°23'16.6"W`
fn parse_dms(s: &str) -> Option<LLA> {
    if !s.contains('°') {
        return None;
    }
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let lat_deg = parse_dms_component(parts[0])?;
    let lon_deg = parse_dms_component(parts[1])?;
    Some(LLA {
        lat: lat_deg.to_radians(),
        lon: lon_deg.to_radians(),
        alt: 0.0,
    })
}

/// Parse decimal format: `37.795200, -122.402800`
fn parse_decimal(s: &str) -> Option<LLA> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 2 {
        return None;
    }
    let lat: f64 = parts[0].trim().parse().ok()?;
    let lon: f64 = parts[1].trim().parse().ok()?;
    Some(LLA {
        lat: lat.to_radians(),
        lon: lon.to_radians(),
        alt: 0.0,
    })
}

// ── ENU→ECEF rotation ─────────────────────────────────────────────────

/// Compute the quaternion that rotates local ENU coordinates to ECEF at the given lat/lon.
fn enu_to_ecef_quat(lat_rad: f64, lon_rad: f64) -> Quat {
    let enu = coords::enu_frame_at(lat_rad, lon_rad, DVec3::ZERO);
    let mat = glam::DMat3::from_cols(enu.east, enu.north, enu.up);
    let dq = glam::DQuat::from_mat3(&mat);
    Quat::from_xyzw(dq.x as f32, dq.y as f32, dq.z as f32, dq.w as f32)
}

// ── Scene loading ──────────────────────────────────────────────────────

/// Load all scene objects: hardcoded teapots + auto-loaded OBJ landmarks.
pub fn load_scene(device: &wgpu::Device, ref_pos: DVec3, enu: &ENUFrame) -> Vec<SceneObject> {
    let mut objects = Vec::new();
    let mut id = 10u32;

    // --- Teapots: hardcoded lineup to the right of runway ---
    let teapot_mesh = obj_loader::load_obj(Path::new("assets/teapot.obj"));
    let teapot_scale = 2.0 / 6.434;

    // Teapot OBJ is Y-up; rotate to ENU Z-up then ENU→ECEF
    let enu_quat = {
        let mat = glam::DMat3::from_cols(enu.east, enu.north, enu.up);
        let dq = glam::DQuat::from_mat3(&mat);
        Quat::from_xyzw(dq.x as f32, dq.y as f32, dq.z as f32, dq.w as f32)
    };
    let y_up_to_z_up = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let teapot_rotation = enu_quat * y_up_to_z_up;

    for i in 0..10 {
        let north_offset = (i as f64) * 100.0;
        let offset_enu = DVec3::new(10.0, north_offset, 0.0); // 10m east of centerline
        let pos = ref_pos + enu.enu_to_ecef(offset_enu);
        objects.push(spawn(
            device,
            &teapot_mesh,
            &format!("teapot_{}", i),
            pos,
            teapot_rotation,
            teapot_scale as f32,
            id,
        ));
        id += 1;
    }

    // --- Auto-load landmarks from assets/ with # origin: tags ---
    let skip = [
        "14082_WWII_Plane_Japan_Kawasaki_Ki-61_v1_L2.obj",
        "teapot.obj",
    ];

    let mut entries: Vec<_> = fs::read_dir("assets")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.path().extension().map_or(false, |ext| ext == "obj")
                && !skip.iter().any(|s| e.file_name().to_string_lossy() == *s)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        if let Some(lla) = parse_origin(&path) {
            let ecef_pos = coords::lla_to_ecef(&lla);
            let rotation = enu_to_ecef_quat(lla.lat, lla.lon);
            let mesh = obj_loader::load_obj(&path);
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            log::info!(
                "Loaded landmark '{}' at ({:.4}\u{00b0}, {:.4}\u{00b0})",
                name,
                lla.lat.to_degrees(),
                lla.lon.to_degrees()
            );
            objects.push(spawn(device, &mesh, &name, ecef_pos, rotation, 1.0, id));
            id += 1;
        }
    }

    objects
}
