use glam::{DVec3, Mat4, Quat};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use wgpu::util::DeviceExt;

use crate::coords::{self, LLA};
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
        "assets/obj_static/14082_WWII_Plane_Japan_Kawasaki_Ki-61_v1_L2.obj",
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

// ── Convention enum ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum ObjConvention {
    Enu, // X=east, Y=north, Z=up (our custom landmarks)
    Yup, // standard modeling Y-up
}

// ── Origin + convention tag parser ───────────────────────────────────

/// Parse `# origin:` and `# convention:` comments from the first 15 lines of an OBJ file.
fn parse_obj_metadata(path: &Path) -> (Option<LLA>, ObjConvention) {
    let mut origin = None;
    let mut convention = ObjConvention::Enu;

    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return (None, convention),
    };
    let reader = BufReader::new(file);

    for line in reader.lines().take(15).flatten() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("# origin:") {
            let rest = rest.trim();
            origin = parse_dms(rest).or_else(|| parse_decimal(rest));
        }
        if let Some(rest) = trimmed.strip_prefix("# convention:") {
            match rest.trim() {
                "yup" => convention = ObjConvention::Yup,
                _ => convention = ObjConvention::Enu,
            }
        }
    }

    (origin, convention)
}

/// Parse a single DMS component like `37°36'57.5"N` → signed degrees.
fn parse_dms_component(s: &str) -> Option<f64> {
    let s = s.trim();
    let direction = s.chars().last()?;
    if !matches!(direction, 'N' | 'S' | 'E' | 'W') {
        return None;
    }
    let s = &s[..s.len() - direction.len_utf8()];

    // Find degree symbol: Unicode U+00B0
    let deg_end = s.find('\u{00B0}')?;
    let deg_symbol_len = '\u{00B0}'.len_utf8(); // 2 bytes in UTF-8
    let min_end = s.find('\'')?;
    let sec_end = s.find('"')?;

    let degrees: f64 = s[..deg_end].parse().ok()?;
    let minutes: f64 = s[deg_end + deg_symbol_len..min_end].parse().ok()?;
    let seconds: f64 = s[min_end + 1..sec_end].parse().ok()?;

    let mut value = degrees + minutes / 60.0 + seconds / 3600.0;
    if direction == 'S' || direction == 'W' {
        value = -value;
    }
    Some(value)
}

/// Parse DMS format: `37°36'57.5"N 122°23'16.6"W`
fn parse_dms(s: &str) -> Option<LLA> {
    if !s.contains('\u{00B0}') {
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

// ── Rotation helpers ─────────────────────────────────────────────────

/// Compute the rotation for an object based on its position and OBJ convention.
fn object_rotation(lla: &LLA, convention: &ObjConvention) -> Quat {
    let enu_quat = enu_to_ecef_quat(lla.lat, lla.lon);
    match convention {
        ObjConvention::Enu => enu_quat,
        ObjConvention::Yup => {
            // Y-up to Z-up: rotate -90° around X (Y becomes Z, Z becomes -Y)
            let y_to_z = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            enu_quat * y_to_z
        }
    }
}

// ── Scene loading ──────────────────────────────────────────────────────

/// Load all scene objects from assets/ using `# origin:` and `# convention:` tags.
pub fn load_scene(device: &wgpu::Device) -> Vec<SceneObject> {
    let mut objects = Vec::new();
    let mut id = 10u32;

    let skip = [
        "14082_WWII_Plane_Japan_Kawasaki_Ki-61_v1_L2.obj",
    ];

    let mut entries: Vec<_> = fs::read_dir("assets/obj_static")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.path().extension().map_or(false, |ext| ext == "obj")
                && !skip.iter().any(|s| e.file_name().to_string_lossy() == *s)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    // Reference: aircraft start position for distance logging
    let ref_lla = LLA {
        lat: 37.613931_f64.to_radians(),
        lon: (-122.358089_f64).to_radians(),
        alt: 2.0,
    };
    let ref_ecef = coords::lla_to_ecef(&ref_lla);

    for entry in entries {
        let path = entry.path();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let (origin, convention) = parse_obj_metadata(&path);
        if let Some(lla) = origin {
            let ecef_pos = coords::lla_to_ecef(&lla);
            let dist = (ecef_pos - ref_ecef).length();
            let rotation = object_rotation(&lla, &convention);
            let mesh = obj_loader::load_obj(&path);
            println!(
                "[scene] Loaded '{}' at ({:.6}\u{00b0}, {:.6}\u{00b0}) conv={:?} dist_from_aircraft={:.1}m",
                name,
                lla.lat.to_degrees(),
                lla.lon.to_degrees(),
                convention,
                dist,
            );
            objects.push(spawn(device, &mesh, &name, ecef_pos, rotation, 1.0, id));
            id += 1;
        } else {
            println!("[scene] WARNING: No valid # origin: found in '{}', skipping", name);
        }
    }

    println!("[scene] Loaded {} scene objects total", objects.len());

    objects
}
