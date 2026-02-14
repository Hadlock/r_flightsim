//! Renders the closest 1024 airports as pyramid markers.
//!
//! Loads all airport positions from JSON once at startup, then periodically
//! finds the closest 1024 to the camera and maintains SceneObjects for them.

use glam::{DVec3, Quat};
use std::path::Path;

use crate::coords::{self, LLA};
use crate::obj_loader::{self, MeshData};
use crate::scene::{self, SceneObject};

const MAX_MARKERS: usize = 1024;
const UPDATE_INTERVAL_SECS: f64 = 2.0; // Re-sort closest airports every 2 seconds

/// Compact airport position data (pre-computed ECEF).
struct AirportPos {
    ecef: DVec3,
    lat_rad: f64,
    lon_rad: f64,
}

pub struct AirportMarkers {
    /// All airport positions (loaded once at startup).
    airports: Vec<AirportPos>,
    /// Indices into `airports` for the current closest set.
    closest_indices: Vec<usize>,
    /// Scene object indices in the main objects array.
    scene_indices: Vec<usize>,
    /// Shared pyramid mesh (uploaded once, buffers cloned per object).
    pyramid_mesh: MeshData,
    /// Time since last closest-set update.
    time_since_update: f64,
    /// Camera position at last update (skip update if barely moved).
    last_update_pos: DVec3,
}

impl AirportMarkers {
    /// Load airport positions from JSON. Returns None if loading fails.
    pub fn new(json_path: &Path) -> Option<Self> {
        let data = match std::fs::read_to_string(json_path) {
            Ok(d) => d,
            Err(e) => {
                log::warn!("Could not read airports JSON for markers: {}", e);
                return None;
            }
        };

        #[derive(serde::Deserialize)]
        struct AirportJson {
            #[serde(rename = "type", default)]
            airport_type: String,
            latitude: f64,
            longitude: f64,
            elevation_ft: Option<f64>,
        }

        let airports_json: Vec<AirportJson> = match serde_json::from_str(&data) {
            Ok(a) => a,
            Err(e) => {
                log::warn!("Could not parse airports JSON for markers: {}", e);
                return None;
            }
        };

        let airports: Vec<AirportPos> = airports_json
            .iter()
            .filter(|a| a.airport_type != "heliport" && a.airport_type != "closed")
            .map(|a| {
                let lat_rad = a.latitude.to_radians();
                let lon_rad = a.longitude.to_radians();
                let alt = a.elevation_ft.unwrap_or(0.0) * 0.3048;
                let ecef = coords::lla_to_ecef(&LLA {
                    lat: lat_rad,
                    lon: lon_rad,
                    alt,
                });
                AirportPos {
                    ecef,
                    lat_rad,
                    lon_rad,
                }
            })
            .collect();

        log::info!(
            "[airport_markers] Loaded {} airport positions for proximity markers",
            airports.len()
        );

        let pyramid_mesh = obj_loader::load_obj(Path::new("assets/obj_static/pyramid_giza.obj"));

        Some(Self {
            airports,
            closest_indices: Vec::new(),
            scene_indices: Vec::new(),
            pyramid_mesh,
            time_since_update: f64::MAX, // Force immediate first update
            last_update_pos: DVec3::new(f64::MAX, 0.0, 0.0),
        })
    }

    /// Create SceneObjects for the markers. Call once during init.
    /// Returns the objects and base index into the objects array.
    pub fn create_scene_objects(
        &mut self,
        device: &wgpu::Device,
        base_object_id: u32,
        objects_base_idx: usize,
    ) -> Vec<SceneObject> {
        let mut scene_objects = Vec::with_capacity(MAX_MARKERS);
        self.scene_indices.clear();

        for i in 0..MAX_MARKERS {
            let obj = scene::spawn(
                device,
                &self.pyramid_mesh,
                "airport_marker",
                DVec3::ZERO,
                Quat::IDENTITY,
                20.0, // 20x scale: ~4.6km base, 3km tall — visible from altitude
                base_object_id + i as u32,
            );
            self.scene_indices.push(objects_base_idx + i);
            scene_objects.push(obj);
        }

        scene_objects
    }

    /// Update marker positions. Call each frame with dt.
    pub fn update(
        &mut self,
        dt: f64,
        camera_ecef: DVec3,
        objects: &mut [SceneObject],
    ) {
        self.time_since_update += dt;

        // Only re-sort every UPDATE_INTERVAL_SECS or if camera moved >10km
        let camera_moved = (camera_ecef - self.last_update_pos).length();
        if self.time_since_update < UPDATE_INTERVAL_SECS && camera_moved < 10_000.0 {
            return;
        }

        self.time_since_update = 0.0;
        self.last_update_pos = camera_ecef;

        // Compute squared distances and find closest MAX_MARKERS
        let mut dists: Vec<(usize, f64)> = self
            .airports
            .iter()
            .enumerate()
            .map(|(i, a)| (i, (a.ecef - camera_ecef).length_squared()))
            .collect();

        // Partial sort: only need the smallest MAX_MARKERS
        let n = MAX_MARKERS.min(dists.len());
        dists.select_nth_unstable_by(n.saturating_sub(1), |a, b| a.1.partial_cmp(&b.1).unwrap());

        self.closest_indices.clear();
        self.closest_indices.extend(dists[..n].iter().map(|(i, _)| *i));

        // Update scene objects
        for (slot, &airport_idx) in self.closest_indices.iter().enumerate() {
            let scene_idx = self.scene_indices[slot];
            let ap = &self.airports[airport_idx];
            objects[scene_idx].world_pos = ap.ecef;
            objects[scene_idx].rotation = enu_to_ecef_quat(ap.lat_rad, ap.lon_rad);
            objects[scene_idx].index_count =
                self.pyramid_mesh.indices.len() as u32;
        }

        // Hide unused slots
        for slot in n..MAX_MARKERS {
            let scene_idx = self.scene_indices[slot];
            objects[scene_idx].index_count = 0;
        }
    }
}

/// ENU-to-ECEF rotation quaternion at a given lat/lon.
fn enu_to_ecef_quat(lat_rad: f64, lon_rad: f64) -> Quat {
    let enu = coords::enu_frame_at(lat_rad, lon_rad, DVec3::ZERO);
    let mat = glam::DMat3::from_cols(enu.east, enu.north, enu.up);
    let dq = glam::DQuat::from_mat3(&mat);
    Quat::from_xyzw(dq.x as f32, dq.y as f32, dq.z as f32, dq.w as f32)
}
