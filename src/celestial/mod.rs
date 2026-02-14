pub mod bodies;
pub mod moon;
pub mod planets;
pub mod stars;
pub mod sun;
pub mod time;

use glam::DVec3;
use wgpu::util::DeviceExt;

use crate::coords;
use crate::obj_loader::{MeshData, Vertex};
use crate::scene::SceneObject;

use self::bodies::{build_merged_cubes, build_moon_mesh, build_sun_mesh};
use self::moon::moon_position;
use self::planets::compute_geocentric_positions;
use self::stars::{star_angular_size, stars_visible, STAR_CATALOG};
use self::sun::sun_position;
use self::time::{gmst_deg, jd_to_t, SimClock};

// ── Coordinate transforms ───────────────────────────────────────────

/// Mean obliquity of the ecliptic at epoch T (Julian centuries from J2000).
pub fn obliquity_deg(t: f64) -> f64 {
    23.439_291 - 0.013_004_2 * t
}

/// Convert ecliptic (lon, lat, dist) to equatorial J2000 Cartesian.
pub fn ecliptic_to_equatorial(lon_rad: f64, lat_rad: f64, dist: f64, obliquity_rad: f64) -> DVec3 {
    let x = dist * lat_rad.cos() * lon_rad.cos();
    let y = dist * lat_rad.cos() * lon_rad.sin();
    let z = dist * lat_rad.sin();
    let cos_e = obliquity_rad.cos();
    let sin_e = obliquity_rad.sin();
    DVec3::new(x, y * cos_e - z * sin_e, y * sin_e + z * cos_e)
}

/// Rotate from J2000 equatorial (ECI) to ECEF using GMST.
pub fn eci_to_ecef(eci: DVec3, gmst_rad: f64) -> DVec3 {
    let cos_g = gmst_rad.cos();
    let sin_g = gmst_rad.sin();
    DVec3::new(
        eci.x * cos_g + eci.y * sin_g,
        -eci.x * sin_g + eci.y * cos_g,
        eci.z,
    )
}

// ── Star toggle ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StarToggleState {
    ProminentOnly,
    AllStars,
    Off,
}

impl StarToggleState {
    pub fn cycle(self) -> Self {
        match self {
            Self::ProminentOnly => Self::AllStars,
            Self::AllStars => Self::Off,
            Self::Off => Self::ProminentOnly,
        }
    }
}

// ── Constants ───────────────────────────────────────────────────────

const SUN_ANGULAR_DIAMETER_RAD: f64 = 0.00930; // 0.533 degrees
const SUN_SUBDIVISIONS: u32 = 6;

const MOON_TRUE_RENDER_THRESHOLD: f64 = 1_000_000_000.0; // 1M km
const MOON_DIAMETER: f64 = 3_474_800.0;
const MOON_LOD_THRESHOLDS: [(f64, u32); 4] = [
    (1_000_000_000.0, 3), // >1M km: sub 3
    (100_000_000.0, 4),   // 100K-1M km: sub 4
    (10_000_000.0, 5),    // 10K-100K km: sub 5
    (0.0, 6),             // <10K km: sub 6
];

const PLANET_ANGULAR_SIZE_RAD: f64 = 0.000_873; // ~0.05 degrees

const EARTH_MEAN_RADIUS: f64 = 6_371_000.0; // meters

/// Check if a direction from the camera is occluded by the earth.
/// Returns true if the ray from camera in `dir` (unit vector) intersects the earth sphere.
fn earth_occludes(camera_ecef: DVec3, dir: DVec3) -> bool {
    let dist_to_center = camera_ecef.length();
    if dist_to_center < EARTH_MEAN_RADIUS * 1.01 {
        return false; // On or near surface, skip occlusion
    }
    // Cosine of the angle from camera-to-earth-center to the earth's limb
    let cos_limb = (1.0 - (EARTH_MEAN_RADIUS / dist_to_center).powi(2)).sqrt();
    // Direction from camera to earth center
    let to_earth = (-camera_ecef).normalize();
    // If dot product > cos_limb, the direction is within the earth's disc
    dir.dot(to_earth) > cos_limb
}

/// Fixed render distance for celestial angular-size trick.
/// Must be well inside even the smallest far plane (40 km at ground level)
/// but far enough that angular-size geometry is precise.
const CELESTIAL_RENDER_DISTANCE: f64 = 30_000.0; // 30 km — always inside far plane

// ── CelestialEngine ─────────────────────────────────────────────────

pub struct CelestialEngine {
    pub clock: SimClock,

    // Cached ECEF positions
    pub sun_ecef: DVec3,
    pub moon_ecef: DVec3,
    pub moon_distance_m: f64,
    pub planet_ecef: [DVec3; 7],

    // Star direction vectors in ECEF (unit vectors)
    prominent_dirs_ecef: Vec<DVec3>,
    prominent_sizes: Vec<f64>,
    other_dirs_ecef: Vec<DVec3>,
    other_sizes: Vec<f64>,

    // Observer-dependent
    pub sun_altitude_deg: f64,
    pub gmst_rad: f64,

    // Toggle
    pub star_toggle: StarToggleState,

    // Mesh templates
    sun_mesh: MeshData,
    moon_meshes: [MeshData; 4], // subdivisions 3,4,5,6
    current_moon_lod: usize,
    cube_mesh: MeshData,

    // Timing
    last_update_jd: f64,
    positions_dirty: bool,
}

impl CelestialEngine {
    pub fn new(epoch_unix: Option<f64>) -> Self {
        let clock = SimClock::new(epoch_unix);
        let jd = clock.jd();
        let t = jd_to_t(jd);
        let gmst_rad = gmst_deg(jd).to_radians();

        // Compute initial positions
        let sun_result = sun_position(jd);
        let sun_ecef = eci_to_ecef(sun_result.eci, gmst_rad);

        let moon_result = moon_position(jd);
        let moon_ecef = eci_to_ecef(moon_result.eci, gmst_rad);

        let planet_eci = compute_geocentric_positions(t);
        let mut planet_ecef = [DVec3::ZERO; 7];
        for (i, eci) in planet_eci.iter().enumerate() {
            planet_ecef[i] = eci_to_ecef(*eci, gmst_rad);
        }

        // Star directions
        let (prominent_dirs, prominent_sizes, other_dirs, other_sizes) =
            compute_star_data(gmst_rad);

        // Generate mesh templates
        let sun_mesh = bodies::generate_icosphere(SUN_SUBDIVISIONS);
        let moon_meshes = [
            bodies::generate_icosphere(3),
            bodies::generate_icosphere(4),
            bodies::generate_icosphere(5),
            bodies::generate_icosphere(6),
        ];
        let cube_mesh = bodies::generate_unit_cube();

        log::info!(
            "[celestial] Initialized: sun at {:.0} km, moon at {:.0} km, {} prominent + {} other stars",
            sun_result.distance_m / 1000.0,
            moon_result.distance_m / 1000.0,
            prominent_dirs.len(),
            other_dirs.len(),
        );

        Self {
            clock,
            sun_ecef,
            moon_ecef,
            moon_distance_m: moon_result.distance_m,
            planet_ecef,
            prominent_dirs_ecef: prominent_dirs,
            prominent_sizes,
            other_dirs_ecef: other_dirs,
            other_sizes,
            sun_altitude_deg: 0.0,
            gmst_rad,
            star_toggle: StarToggleState::ProminentOnly,
            sun_mesh,
            moon_meshes,
            current_moon_lod: 0,
            cube_mesh,
            last_update_jd: jd,
            positions_dirty: true,
        }
    }

    /// Advance clock, recompute positions at ~1 Hz.
    pub fn update(&mut self, dt: f64, observer_ecef: DVec3) {
        self.clock.advance(dt);

        let jd = self.clock.jd();
        let update_interval_jd = 1.0 / 86_400.0; // ~1 second

        if (jd - self.last_update_jd).abs() >= update_interval_jd {
            self.recompute(jd);
            self.last_update_jd = jd;
            self.positions_dirty = true;
        }

        self.update_observer(observer_ecef);
    }

    fn recompute(&mut self, jd: f64) {
        let t = jd_to_t(jd);
        self.gmst_rad = gmst_deg(jd).to_radians();

        let sun_result = sun_position(jd);
        self.sun_ecef = eci_to_ecef(sun_result.eci, self.gmst_rad);

        let moon_result = moon_position(jd);
        self.moon_ecef = eci_to_ecef(moon_result.eci, self.gmst_rad);
        self.moon_distance_m = moon_result.distance_m;

        let planet_eci = compute_geocentric_positions(t);
        for (i, eci) in planet_eci.iter().enumerate() {
            self.planet_ecef[i] = eci_to_ecef(*eci, self.gmst_rad);
        }

        let (prominent_dirs, prominent_sizes, other_dirs, other_sizes) =
            compute_star_data(self.gmst_rad);
        self.prominent_dirs_ecef = prominent_dirs;
        self.prominent_sizes = prominent_sizes;
        self.other_dirs_ecef = other_dirs;
        self.other_sizes = other_sizes;
    }

    fn update_observer(&mut self, observer_ecef: DVec3) {
        // Sun altitude above horizon
        let lla = coords::ecef_to_lla(observer_ecef);
        let enu = coords::enu_frame_at(lla.lat, lla.lon, observer_ecef);
        let sun_dir = (self.sun_ecef - observer_ecef).normalize();
        let sun_enu = enu.ecef_to_enu(sun_dir);
        self.sun_altitude_deg = sun_enu.z.asin().to_degrees();
    }

    /// Create the 5 SceneObjects for celestial bodies.
    /// Returns (objects, [sun_idx, moon_idx, planets_idx, prominent_stars_idx, other_stars_idx]).
    pub fn create_scene_objects(
        &self,
        device: &wgpu::Device,
        base_id: u32,
    ) -> (Vec<SceneObject>, [usize; 5]) {
        let mut objects = Vec::with_capacity(5);

        // We need empty placeholder meshes — they get rebuilt each frame.
        // Create with VERTEX | COPY_DST so we can write_buffer later.
        let sun_obj = create_dynamic_scene_object(device, &self.sun_mesh, "sun", base_id);
        objects.push(sun_obj);

        let moon_obj = create_dynamic_scene_object(
            device,
            &self.moon_meshes[3], // worst-case largest
            "moon",
            base_id + 1,
        );
        objects.push(moon_obj);

        // Planets: 7 cubes merged
        let planet_placeholder =
            build_merged_cubes(&self.cube_mesh, &[DVec3::X; 7], &[0.001; 7], 30000.0, DVec3::ZERO);
        let planets_obj =
            create_dynamic_scene_object(device, &planet_placeholder, "planets", base_id + 2);
        objects.push(planets_obj);

        // Prominent stars
        let star_count_p = self.prominent_dirs_ecef.len().max(1);
        let star_placeholder_p = build_merged_cubes(
            &self.cube_mesh,
            &vec![DVec3::X; star_count_p],
            &vec![0.001; star_count_p],
            30000.0,
            DVec3::ZERO,
        );
        let prominent_obj =
            create_dynamic_scene_object(device, &star_placeholder_p, "stars_prominent", base_id + 3);
        objects.push(prominent_obj);

        // Other stars
        let star_count_o = self.other_dirs_ecef.len().max(1);
        let star_placeholder_o = build_merged_cubes(
            &self.cube_mesh,
            &vec![DVec3::X; star_count_o],
            &vec![0.001; star_count_o],
            30000.0,
            DVec3::ZERO,
        );
        let other_obj =
            create_dynamic_scene_object(device, &star_placeholder_o, "stars_other", base_id + 4);
        objects.push(other_obj);

        let indices = [0, 1, 2, 3, 4];
        (objects, indices)
    }

    /// Update celestial SceneObjects each frame.
    pub fn update_scene_objects(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        objects: &mut [SceneObject],
        indices: &[usize; 5],
        camera_ecef: DVec3,
        altitude_m: f64,
        _far_plane: f32,
    ) {
        let render_distance = CELESTIAL_RENDER_DISTANCE;

        // ── Sun ──
        let sun_dir = (self.sun_ecef - camera_ecef).normalize();
        if earth_occludes(camera_ecef, sun_dir) {
            objects[indices[0]].index_count = 0;
        } else {
            let sun_mesh = build_sun_mesh(
                &self.sun_mesh,
                sun_dir,
                render_distance,
                SUN_ANGULAR_DIAMETER_RAD,
            );
            update_dynamic_mesh(device, queue, &mut objects[indices[0]], &sun_mesh);
            objects[indices[0]].world_pos = camera_ecef;
        }

        // ── Moon ──
        let moon_dir = (self.moon_ecef - camera_ecef).normalize();
        if earth_occludes(camera_ecef, moon_dir) {
            objects[indices[1]].index_count = 0;
        } else {
            let moon_dist = (self.moon_ecef - camera_ecef).length();
            let moon_lod = select_moon_lod(moon_dist);
            if moon_lod != self.current_moon_lod {
                self.current_moon_lod = moon_lod;
            }
            let moon_mesh = build_moon_mesh(
                &self.moon_meshes[moon_lod],
                self.moon_ecef,
                self.moon_distance_m,
                MOON_DIAMETER,
                camera_ecef,
                render_distance,
                MOON_TRUE_RENDER_THRESHOLD,
            );
            update_dynamic_mesh(device, queue, &mut objects[indices[1]], &moon_mesh);
            objects[indices[1]].world_pos = camera_ecef;
        }

        // ── Planets (filter occluded) ──
        let mut planet_dirs = Vec::with_capacity(7);
        let mut planet_sizes = Vec::with_capacity(7);
        for p in &self.planet_ecef {
            let dir = (*p - camera_ecef).normalize();
            if !earth_occludes(camera_ecef, dir) {
                planet_dirs.push(dir);
                planet_sizes.push(PLANET_ANGULAR_SIZE_RAD);
            }
        }
        if planet_dirs.is_empty() {
            objects[indices[2]].index_count = 0;
        } else {
            let planet_mesh = build_merged_cubes(
                &self.cube_mesh,
                &planet_dirs,
                &planet_sizes,
                render_distance,
                camera_ecef,
            );
            update_dynamic_mesh(device, queue, &mut objects[indices[2]], &planet_mesh);
            objects[indices[2]].world_pos = camera_ecef;
        }

        // ── Stars (filter occluded) ──
        let show_stars = stars_visible(self.sun_altitude_deg, altitude_m);

        // Prominent stars
        match self.star_toggle {
            StarToggleState::Off => {
                objects[indices[3]].index_count = 0;
            }
            _ if !show_stars => {
                objects[indices[3]].index_count = 0;
            }
            _ => {
                let (dirs, sizes): (Vec<_>, Vec<_>) = self
                    .prominent_dirs_ecef
                    .iter()
                    .zip(self.prominent_sizes.iter())
                    .filter(|(d, _)| !earth_occludes(camera_ecef, **d))
                    .unzip();
                if dirs.is_empty() {
                    objects[indices[3]].index_count = 0;
                } else {
                    let star_mesh_p = build_merged_cubes(
                        &self.cube_mesh,
                        &dirs,
                        &sizes,
                        render_distance,
                        camera_ecef,
                    );
                    update_dynamic_mesh(device, queue, &mut objects[indices[3]], &star_mesh_p);
                    objects[indices[3]].world_pos = camera_ecef;
                }
            }
        }

        // Other stars
        match self.star_toggle {
            StarToggleState::AllStars if show_stars => {
                let (dirs, sizes): (Vec<_>, Vec<_>) = self
                    .other_dirs_ecef
                    .iter()
                    .zip(self.other_sizes.iter())
                    .filter(|(d, _)| !earth_occludes(camera_ecef, **d))
                    .unzip();
                if dirs.is_empty() {
                    objects[indices[4]].index_count = 0;
                } else {
                    let star_mesh_o = build_merged_cubes(
                        &self.cube_mesh,
                        &dirs,
                        &sizes,
                        render_distance,
                        camera_ecef,
                    );
                    update_dynamic_mesh(device, queue, &mut objects[indices[4]], &star_mesh_o);
                    objects[indices[4]].world_pos = camera_ecef;
                }
            }
            _ => {
                objects[indices[4]].index_count = 0;
            }
        }

        self.positions_dirty = false;
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

fn select_moon_lod(distance_m: f64) -> usize {
    for (i, &(threshold, _)) in MOON_LOD_THRESHOLDS.iter().enumerate() {
        if distance_m > threshold {
            return i;
        }
    }
    MOON_LOD_THRESHOLDS.len() - 1
}

fn compute_star_data(gmst_rad: f64) -> (Vec<DVec3>, Vec<f64>, Vec<DVec3>, Vec<f64>) {
    let mut prominent_dirs = Vec::new();
    let mut prominent_sizes = Vec::new();
    let mut other_dirs = Vec::new();
    let mut other_sizes = Vec::new();

    for star in STAR_CATALOG {
        let ra = star.ra_deg.to_radians();
        let dec = star.dec_deg.to_radians();
        let eci = DVec3::new(dec.cos() * ra.cos(), dec.cos() * ra.sin(), dec.sin());
        let ecef_dir = eci_to_ecef(eci, gmst_rad);
        let ang_size = star_angular_size(star.mag);

        if star.prominent {
            prominent_dirs.push(ecef_dir);
            prominent_sizes.push(ang_size);
        } else {
            other_dirs.push(ecef_dir);
            other_sizes.push(ang_size);
        }
    }

    (prominent_dirs, prominent_sizes, other_dirs, other_sizes)
}

/// Create a SceneObject with COPY_DST vertex buffer for dynamic updates.
fn create_dynamic_scene_object(
    device: &wgpu::Device,
    mesh: &MeshData,
    name: &str,
    object_id: u32,
) -> SceneObject {
    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("{} Vertex Buffer", name)),
        contents: bytemuck::cast_slice(&mesh.vertices),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    });
    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("{} Index Buffer", name)),
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
    });

    SceneObject {
        name: name.to_string(),
        vertex_buf,
        index_buf,
        index_count: mesh.indices.len() as u32,
        world_pos: DVec3::ZERO,
        rotation: glam::Quat::IDENTITY,
        scale: 1.0,
        object_id,
        edges_enabled: true,
        bounding_radius: f32::MAX, // celestial objects should never be culled by bounding
    }
}

/// Update a dynamic SceneObject's vertex and index buffers.
fn update_dynamic_mesh(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    obj: &mut SceneObject,
    mesh: &MeshData,
) {
    let vert_bytes = bytemuck::cast_slice::<Vertex, u8>(&mesh.vertices);
    let idx_bytes = bytemuck::cast_slice::<u32, u8>(&mesh.indices);

    // Check if existing buffer is large enough
    if vert_bytes.len() as u64 <= obj.vertex_buf.size() {
        queue.write_buffer(&obj.vertex_buf, 0, vert_bytes);
    } else {
        obj.vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{} Vertex Buffer", obj.name)),
            contents: vert_bytes,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
    }

    if idx_bytes.len() as u64 <= obj.index_buf.size() {
        queue.write_buffer(&obj.index_buf, 0, idx_bytes);
    } else {
        obj.index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{} Index Buffer", obj.name)),
            contents: idx_bytes,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });
    }

    obj.index_count = mesh.indices.len() as u32;
}
