//! Procedural airport geometry generator.
//!
//! Reads `assets/airports/all_airports.json` and, for every non-heliport airport,
//! produces SceneObjects for:
//!   - Each runway  (flat rectangle at field elevation)
//!   - ATC tower    (10×10×30 m)
//!   - Hangars      (count depends on airport size)
//!   - Admin buildings
//!   - Aux buildings (1–32 seeded by ident hash)
//!
//! All geometry is created in memory (no .obj files on disk).

use glam::{DVec3, Quat};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::coords::{self, LLA};
use crate::obj_loader::{MeshData, Vertex};
use crate::scene::SceneObject;

// ── JSON schema ──────────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Debug)]
#[allow(dead_code)]
struct AirportJson {
    ident: String,
    #[serde(default)]
    name: String,
    #[serde(rename = "type", default)]
    airport_type: String,
    latitude: f64,
    longitude: f64,
    elevation_ft: Option<f64>,
    runways: Option<Vec<RunwayJson>>,
}

#[derive(serde::Deserialize, Debug)]
#[allow(dead_code, non_snake_case)]
struct RunwayJson {
    length_ft: Option<f64>,
    width_ft: Option<f64>,
    surface: Option<String>,
    lighted: Option<bool>,
    closed: Option<bool>,
    le_ident: Option<String>,
    le_heading_degT: Option<f64>,
    he_heading_degT: Option<f64>,
}

impl RunwayJson {
    /// Get the heading in degrees, falling back to inferring from le_ident (e.g. "09" → 90°).
    fn heading_deg(&self) -> Option<f64> {
        if let Some(h) = self.le_heading_degT {
            return Some(h);
        }
        // Try to parse numeric prefix from le_ident (e.g. "09L" → 9 → 90°)
        if let Some(ident) = &self.le_ident {
            let num_str: String = ident.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(num) = num_str.parse::<u32>() {
                if num >= 1 && num <= 36 {
                    return Some(num as f64 * 10.0);
                }
            }
        }
        None
    }
}

// ── Geometry helpers ─────────────────────────────────────────────────────────

const FT_TO_M: f64 = 0.3048;

/// Create a box mesh centred at the origin. Width along X, depth along Y, height along Z-up.
/// Vertices are in a local ENU-like frame (X=east, Y=north, Z=up).
fn make_box_mesh(width: f32, depth: f32, height: f32) -> MeshData {
    let hw = width * 0.5;
    let hd = depth * 0.5;
    // bottom at z=0, top at z=height
    let z0 = 0.0_f32;
    let z1 = height;

    // 8 corners
    let corners = [
        [-hw, -hd, z0], // 0: bottom SW
        [ hw, -hd, z0], // 1: bottom SE
        [ hw,  hd, z0], // 2: bottom NE
        [-hw,  hd, z0], // 3: bottom NW
        [-hw, -hd, z1], // 4: top SW
        [ hw, -hd, z1], // 5: top SE
        [ hw,  hd, z1], // 6: top NE
        [-hw,  hd, z1], // 7: top NW
    ];

    // 6 faces, each 2 triangles, with outward normals
    struct Face {
        verts: [usize; 4], // CCW when viewed from outside
        normal: [f32; 3],
    }

    let faces = [
        Face { verts: [3, 2, 1, 0], normal: [0.0, 0.0, -1.0] }, // bottom
        Face { verts: [4, 5, 6, 7], normal: [0.0, 0.0,  1.0] }, // top
        Face { verts: [0, 1, 5, 4], normal: [0.0, -1.0, 0.0] }, // south (-Y)
        Face { verts: [2, 3, 7, 6], normal: [0.0,  1.0, 0.0] }, // north (+Y)
        Face { verts: [3, 0, 4, 7], normal: [-1.0, 0.0, 0.0] }, // west (-X)
        Face { verts: [1, 2, 6, 5], normal: [ 1.0, 0.0, 0.0] }, // east (+X)
    ];

    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    for face in &faces {
        let base = vertices.len() as u32;
        for &vi in &face.verts {
            vertices.push(Vertex {
                position: corners[vi],
                normal: face.normal,
            });
        }
        // Two triangles: 0-1-2, 0-2-3
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    MeshData { vertices, indices }
}

/// Create a flat rectangle (very thin box) for a runway.
fn make_runway_mesh(width_m: f32, length_m: f32) -> MeshData {
    // Runways are essentially flat — 0.3m thick so edges render
    make_box_mesh(width_m, length_m, 0.3)
}

/// Rotate all vertices in a mesh around the Z axis (ENU up) by `angle_rad`.
fn rotate_mesh_z(mesh: &mut MeshData, angle_rad: f32) {
    let (s, c) = angle_rad.sin_cos();
    for v in &mut mesh.vertices {
        let x = v.position[0];
        let y = v.position[1];
        v.position[0] = x * c - y * s;
        v.position[1] = x * s + y * c;
        let nx = v.normal[0];
        let ny = v.normal[1];
        v.normal[0] = nx * c - ny * s;
        v.normal[1] = nx * s + ny * c;
    }
}

/// Translate all vertices by (dx, dy, dz).
fn translate_mesh(mesh: &mut MeshData, dx: f32, dy: f32, dz: f32) {
    for v in &mut mesh.vertices {
        v.position[0] += dx;
        v.position[1] += dy;
        v.position[2] += dz;
    }
}

/// Merge `other` into `base`.
fn merge_mesh(base: &mut MeshData, other: &MeshData) {
    let offset = base.vertices.len() as u32;
    base.vertices.extend_from_slice(&other.vertices);
    for &idx in &other.indices {
        base.indices.push(offset + idx);
    }
}

// ── Collision / placement ────────────────────────────────────────────────────

/// Axis-aligned bounding box in the local ENU plane (ignoring Z for overlap).
#[derive(Clone, Debug)]
struct Footprint {
    cx: f64,
    cy: f64,
    half_w: f64, // half-extent along the footprint's local X after rotation
    half_d: f64, // half-extent along the footprint's local Y after rotation
    angle: f64,  // rotation angle (rad) around Z for OBB check
}

impl Footprint {
    /// Oriented-bounding-box overlap test using Separating Axis Theorem (2D).
    fn overlaps(&self, other: &Footprint) -> bool {
        // Get the 4 corners of each OBB, then do SAT with 4 axes.
        let corners_a = self.corners();
        let corners_b = other.corners();
        let axes = self.axes().into_iter().chain(other.axes());
        for axis in axes {
            let (min_a, max_a) = project(&corners_a, axis);
            let (min_b, max_b) = project(&corners_b, axis);
            if max_a < min_b || max_b < min_a {
                return false; // separating axis found
            }
        }
        true
    }

    fn corners(&self) -> [(f64, f64); 4] {
        let (s, c) = self.angle.sin_cos();
        let hw = self.half_w;
        let hd = self.half_d;
        let offsets = [(-hw, -hd), (hw, -hd), (hw, hd), (-hw, hd)];
        offsets.map(|(lx, ly)| {
            (self.cx + lx * c - ly * s, self.cy + lx * s + ly * c)
        })
    }

    fn axes(&self) -> [(f64, f64); 2] {
        let (s, c) = self.angle.sin_cos();
        [(c, s), (-s, c)]
    }
}

fn project(corners: &[(f64, f64); 4], axis: (f64, f64)) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &(cx, cy) in corners {
        let d = cx * axis.0 + cy * axis.1;
        if d < min { min = d; }
        if d > max { max = d; }
    }
    (min, max)
}

// ── Deterministic hash helpers ───────────────────────────────────────────────

fn ident_hash(ident: &str) -> u64 {
    let mut h = DefaultHasher::new();
    ident.hash(&mut h);
    h.finish()
}

/// Pseudo-random f64 in [lo, hi) seeded by a hash value.
fn hash_range(hash: u64, lo: f64, hi: f64) -> f64 {
    let t = (hash as f64) / (u64::MAX as f64);
    lo + t * (hi - lo)
}

fn sub_hash(base: u64, index: u32) -> u64 {
    let mut h = DefaultHasher::new();
    base.hash(&mut h);
    index.hash(&mut h);
    h.finish()
}

// ── Building placement ───────────────────────────────────────────────────────

#[allow(dead_code)]
struct BuildingSpec {
    width: f64,  // ENU X extent (m)
    depth: f64,  // ENU Y extent (m)
    height: f64, // Z extent (m)
    label: &'static str,
}

/// Try to place a building near the runway cluster. Returns footprint + ENU offset if successful.
fn try_place_building(
    spec: &BuildingSpec,
    placed: &[Footprint],
    runway_footprints: &[Footprint],
    runway_angle: f64,
    side_sign: f64, // +1.0 or -1.0 to pick side of runway
    attempt_seed: u64,
    max_lateral: f64,
    max_along: f64,
) -> Option<(Footprint, f64, f64)> {
    // Try several random positions on the chosen side of the runway
    for attempt in 0..40u32 {
        let h1 = sub_hash(attempt_seed, attempt * 2);
        let h2 = sub_hash(attempt_seed, attempt * 2 + 1);

        // Distance perpendicular to runway (away from centreline)
        let perp_dist = hash_range(h1, 60.0, max_lateral);
        // Distance along runway direction
        let along_dist = hash_range(h2, -max_along, max_along);

        let (s, c) = runway_angle.sin_cos();
        // Perpendicular direction: rotate runway heading 90°
        let px = -s * side_sign;
        let py = c * side_sign;
        // Along direction
        let ax = c;
        let ay = s;

        let cx = perp_dist * px + along_dist * ax;
        let cy = perp_dist * py + along_dist * ay;

        // Align building with runway
        let fp = Footprint {
            cx,
            cy,
            half_w: spec.width * 0.5 + 2.0, // 2m padding
            half_d: spec.depth * 0.5 + 2.0,
            angle: runway_angle,
        };

        // Check overlap with runways
        let rwy_ok = runway_footprints.iter().all(|r| !fp.overlaps(r));
        // Check overlap with already placed buildings (skip ATC tower — index 0 if present)
        let bld_ok = placed.iter().all(|p| !fp.overlaps(p));

        if rwy_ok && bld_ok {
            return Some((fp, cx, cy));
        }
    }
    None
}

// ── Main entry point ─────────────────────────────────────────────────────────

/// Max distance (metres) from reference position to generate airports.
const LOAD_RADIUS_M: f64 = 200_000.0; // 200 km

/// Compact airport position extracted from parsed JSON — shared with airport_markers.
pub struct AirportPosition {
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub elevation_ft: f64,
}

/// Parsed airport data — holds the result of the single JSON parse.
pub struct ParsedAirports {
    airports: Vec<AirportJson>,
}

/// Parse the airports JSON once. Returns parsed data for both generate_airports and markers.
pub fn parse_airports_json(json_data: &str) -> ParsedAirports {
    let airports: Vec<AirportJson> = match serde_json::from_str(json_data) {
        Ok(a) => a,
        Err(e) => {
            log::warn!("Could not parse airports JSON: {}", e);
            return ParsedAirports { airports: Vec::new() };
        }
    };
    ParsedAirports { airports }
}

impl ParsedAirports {
    /// Extract positions for airport_markers (no runways needed).
    pub fn positions(&self) -> Vec<AirportPosition> {
        self.airports
            .iter()
            .filter(|a| a.airport_type != "heliport" && a.airport_type != "closed")
            .map(|a| AirportPosition {
                lat_deg: a.latitude,
                lon_deg: a.longitude,
                elevation_ft: a.elevation_ft.unwrap_or(0.0),
            })
            .collect()
    }
}

/// Load nearby airports from pre-parsed data and generate SceneObjects.
/// Only airports within `LOAD_RADIUS_M` of `ref_ecef` are generated.
/// `next_object_id` is the starting object_id; returns (objects, next_id_after).
pub fn generate_airports(
    device: &wgpu::Device,
    parsed: &ParsedAirports,
    next_object_id: u32,
    ref_ecef: DVec3,
) -> (Vec<SceneObject>, u32) {
    let airports = &parsed.airports;

    let mut objects = Vec::new();
    let mut obj_id = next_object_id;

    for airport in airports {
        // Skip heliports and closed airports
        if airport.airport_type == "heliport" || airport.airport_type == "closed" {
            continue;
        }

        let runways = match &airport.runways {
            Some(r) if !r.is_empty() => r,
            _ => continue,
        };

        // Quick distance check (spherical approximation) — skip far airports
        let elev_m_quick = airport.elevation_ft.unwrap_or(0.0) * FT_TO_M;
        let apt_ecef_quick = coords::lla_to_ecef(&LLA {
            lat: airport.latitude.to_radians(),
            lon: airport.longitude.to_radians(),
            alt: elev_m_quick,
        });
        if (apt_ecef_quick - ref_ecef).length() > LOAD_RADIUS_M {
            continue;
        }

        // Filter out closed runways and those missing dimensions/heading
        let valid_runways: Vec<&RunwayJson> = runways
            .iter()
            .filter(|r| {
                !r.closed.unwrap_or(false)
                    && r.length_ft.unwrap_or(0.0) > 0.0
                    && r.width_ft.unwrap_or(0.0) > 0.0
                    && r.heading_deg().is_some()
            })
            .collect();

        if valid_runways.is_empty() {
            continue;
        }

        let elev_m = airport.elevation_ft.unwrap_or(0.0) * FT_TO_M;
        let apt_lla = LLA {
            lat: airport.latitude.to_radians(),
            lon: airport.longitude.to_radians(),
            alt: elev_m,
        };
        let apt_ecef = coords::lla_to_ecef(&apt_lla);

        // Compute ENU→ECEF rotation quaternion for this airport
        let enu_quat = enu_to_ecef_quat(apt_lla.lat, apt_lla.lon);

        let mut runway_footprints: Vec<Footprint> = Vec::new();

        // Find the longest runway heading (for building alignment)
        let longest_rwy = valid_runways
            .iter()
            .max_by(|a, b| {
                a.length_ft
                    .unwrap_or(0.0)
                    .partial_cmp(&b.length_ft.unwrap_or(0.0))
                    .unwrap()
            })
            .unwrap();
        let primary_heading_deg = longest_rwy.heading_deg().unwrap_or(0.0);
        let primary_angle_rad = (90.0 - primary_heading_deg).to_radians();

        // ── Group parallel runways and compute lateral offsets ──
        const MIN_PARALLEL_SEP: f64 = 230.0;
        // Offset between heading groups along the primary runway direction,
        // so crossing zones are clearly separated (visible # pattern).
        const GROUP_SPREAD: f64 = 500.0;

        fn normalise_hdg(h: f64) -> f64 {
            ((h % 360.0) + 360.0) % 360.0
        }

        // Group by heading (parallel if within 5°)
        let mut groups: Vec<(f64, Vec<usize>)> = Vec::new();
        for (i, rwy) in valid_runways.iter().enumerate() {
            let hdg = normalise_hdg(rwy.heading_deg().unwrap_or(0.0));
            let mut found = false;
            for (_gi, (group_hdg, members)) in groups.iter_mut().enumerate() {
                let diff = (hdg - *group_hdg + 540.0) % 360.0 - 180.0;
                if diff.abs() < 5.0 {
                    members.push(i);
                    found = true;
                    break;
                }
            }
            if !found {
                groups.push((hdg, vec![i]));
            }
        }

        // Compute ENU offset for each runway, then convert to a separate ECEF position.
        // Each runway becomes its own SceneObject — no vertex translation, just distinct world_pos.
        let enu_frame = coords::enu_frame_at(apt_lla.lat, apt_lla.lon, apt_ecef);

        // Primary heading direction in ENU (used to offset secondary groups)
        let primary_dir_east = primary_angle_rad.cos();
        let primary_dir_north = primary_angle_rad.sin();

        struct RwyInfo {
            idx: usize,
            offset_east: f64,
            offset_north: f64,
        }
        let mut rwy_infos: Vec<RwyInfo> = Vec::new();

        for (gi, (group_hdg, members)) in groups.iter().enumerate() {
            let angle_rad = (90.0 - group_hdg).to_radians();
            // Perpendicular to runway heading in ENU (for L/R separation)
            let perp_east = -(angle_rad.sin());
            let perp_north = angle_rad.cos();

            // Offset secondary groups along the primary heading direction.
            // This shifts the crossing zone away from center, making the #
            // pattern clearly visible instead of all crossings at one point.
            let group_shift = gi as f64 * GROUP_SPREAD;
            let group_offset_east = group_shift * primary_dir_east;
            let group_offset_north = group_shift * primary_dir_north;

            let n = members.len();
            for (rank, &idx) in members.iter().enumerate() {
                let lateral = (rank as f64 - (n as f64 - 1.0) * 0.5) * MIN_PARALLEL_SEP;
                rwy_infos.push(RwyInfo {
                    idx,
                    offset_east: lateral * perp_east + group_offset_east,
                    offset_north: lateral * perp_north + group_offset_north,
                });
            }
        }

        // Create each runway as a separate SceneObject with its own ECEF position
        for ri in &rwy_infos {
            let rwy = &valid_runways[ri.idx];
            let length_m = rwy.length_ft.unwrap_or(0.0) * FT_TO_M;
            let width_m = rwy.width_ft.unwrap_or(0.0) * FT_TO_M;
            let heading_deg = rwy.heading_deg().unwrap_or(0.0);
            let angle_rad = (90.0 - heading_deg).to_radians();

            // Mesh at origin, just rotated to heading
            let mut mesh = make_runway_mesh(width_m as f32, length_m as f32);
            rotate_mesh_z(&mut mesh, angle_rad as f32);

            // Compute this runway's ECEF position from its ENU offset
            let enu_offset = DVec3::new(ri.offset_east, ri.offset_north, 0.0);
            let rwy_ecef = apt_ecef + enu_frame.enu_to_ecef(enu_offset);

            let radius = crate::scene::mesh_bounding_radius(&mesh);
            let bufs = upload_mesh(device, &mesh, &format!("{}_{}", airport.ident,
                rwy.le_ident.as_deref().unwrap_or("rwy")));
            objects.push(SceneObject {
                name: format!("{}_{}", airport.ident,
                    rwy.le_ident.as_deref().unwrap_or("rwy")),
                vertex_buf: bufs.0,
                index_buf: bufs.1,
                index_count: bufs.2,
                world_pos: rwy_ecef,
                rotation: enu_quat,
                scale: 1.0,
                object_id: obj_id,
                edges_enabled: true,
                bounding_radius: radius,
            });
            obj_id += 1;

            // Track footprint in ENU for building placement
            runway_footprints.push(Footprint {
                cx: ri.offset_east,
                cy: ri.offset_north,
                half_w: width_m * 0.5 + 5.0,
                half_d: length_m * 0.5 + 5.0,
                angle: angle_rad,
            });
        }

        // ── Determine building counts by airport size ──
        let size_class = &airport.airport_type;
        let (n_hangar1, n_hangar2, n_admin) = match size_class.as_str() {
            "large_airport" => (6, 4, 8),
            "medium_airport" => (2, 2, 1),
            _ => (1, 1, 1), // small_airport or other
        };

        // Building specs
        let atc = BuildingSpec { width: 10.0, depth: 10.0, height: 120.0, label: "atc" };
        let hangar1 = BuildingSpec { width: 45.0, depth: 80.0, height: 20.0, label: "hangar1" };
        let hangar2 = BuildingSpec { width: 40.0, depth: 70.0, height: 15.0, label: "hangar2" };
        let admin = BuildingSpec { width: 33.0, depth: 33.0, height: 10.0, label: "admin" };

        let h = ident_hash(&airport.ident);

        // Pick a side for the building cluster (+1 or -1)
        let side_sign = if h % 2 == 0 { 1.0 } else { -1.0 };

        let longest_len_m = longest_rwy.length_ft.unwrap_or(3000.0) * FT_TO_M;
        let max_lateral = (longest_len_m * 0.3).max(200.0).min(800.0);
        let max_along = longest_len_m * 0.4;

        let mut placed: Vec<Footprint> = Vec::new();
        let mut building_mesh = MeshData {
            vertices: Vec::new(),
            indices: Vec::new(),
        };

        // Collect all buildings to place
        let mut specs: Vec<&BuildingSpec> = Vec::new();
        specs.push(&atc); // ATC tower first
        for _ in 0..n_hangar1 { specs.push(&hangar1); }
        for _ in 0..n_hangar2 { specs.push(&hangar2); }
        for _ in 0..n_admin { specs.push(&admin); }

        // Aux buildings: 1–32 based on ident hash
        let n_aux = ((h >> 8) % 32 + 1) as u32;
        let mut aux_specs: Vec<BuildingSpec> = Vec::new();
        for i in 0..n_aux {
            let sh = sub_hash(h, 1000 + i);
            let w = hash_range(sh, 10.0, 35.0);
            let sh2 = sub_hash(h, 2000 + i);
            let d = hash_range(sh2, 10.0, 35.0);
            let sh3 = sub_hash(h, 3000 + i);
            let ht = hash_range(sh3, 6.0, 12.0);
            aux_specs.push(BuildingSpec {
                width: w,
                depth: d,
                height: ht,
                label: "aux",
            });
        }
        for s in &aux_specs {
            specs.push(s);
        }

        // Place each building
        for (bi, spec) in specs.iter().enumerate() {
            let seed = sub_hash(h, 5000 + bi as u32);
            if let Some((fp, cx, cy)) = try_place_building(
                spec,
                &placed,
                &runway_footprints,
                primary_angle_rad,
                side_sign,
                seed,
                max_lateral,
                max_along,
            ) {
                placed.push(fp);
                let mut bm = make_box_mesh(spec.width as f32, spec.depth as f32, spec.height as f32);
                rotate_mesh_z(&mut bm, primary_angle_rad as f32);
                translate_mesh(&mut bm, cx as f32, cy as f32, 0.0);
                merge_mesh(&mut building_mesh, &bm);
            }
        }

        // Upload buildings mesh
        if !building_mesh.vertices.is_empty() {
            let radius = crate::scene::mesh_bounding_radius(&building_mesh);
            let bufs = upload_mesh(
                device,
                &building_mesh,
                &format!("{}_buildings", airport.ident),
            );
            objects.push(SceneObject {
                name: format!("{}_buildings", airport.ident),
                vertex_buf: bufs.0,
                index_buf: bufs.1,
                index_count: bufs.2,
                world_pos: apt_ecef,
                rotation: enu_quat,
                scale: 1.0,
                object_id: obj_id,
                edges_enabled: true,
                bounding_radius: radius,
            });
            obj_id += 1;
        }
    }

    log::info!(
        "Generated {} airport scene objects from {}",
        objects.len(),
        "airports_all.json"
    );
    println!(
        "[airport_gen] Generated {} scene objects from {} airports",
        objects.len(),
        airports.iter().filter(|a| a.airport_type != "heliport").count()
    );

    (objects, obj_id)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn enu_to_ecef_quat(lat_rad: f64, lon_rad: f64) -> Quat {
    let enu = coords::enu_frame_at(lat_rad, lon_rad, DVec3::ZERO);
    let mat = glam::DMat3::from_cols(enu.east, enu.north, enu.up);
    let dq = glam::DQuat::from_mat3(&mat);
    Quat::from_xyzw(dq.x as f32, dq.y as f32, dq.z as f32, dq.w as f32)
}

fn upload_mesh(
    device: &wgpu::Device,
    mesh: &MeshData,
    label: &str,
) -> (wgpu::Buffer, wgpu::Buffer, u32) {
    use wgpu::util::DeviceExt;
    let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("{} VB", label)),
        contents: bytemuck::cast_slice(&mesh.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("{} IB", label)),
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    (vb, ib, mesh.indices.len() as u32)
}