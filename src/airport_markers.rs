//! Renders the closest 1024 airports as pyramid markers.
//!
//! Loads all airport positions from JSON once at startup, then periodically
//! finds the closest 1024 to the camera and maintains SceneObjects for them.

use glam::{DVec3, Quat};
use std::path::Path;

use crate::airport_gen::AirportPosition;
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
    /// Create from pre-parsed airport positions (avoids re-parsing the 43MB JSON).
    pub fn new(positions: &[AirportPosition]) -> Option<Self> {
        if positions.is_empty() {
            return None;
        }

        let airports: Vec<AirportPos> = positions
            .iter()
            .map(|a| {
                let lat_rad = a.lat_deg.to_radians();
                let lon_rad = a.lon_deg.to_radians();
                let alt = a.elevation_ft * crate::constants::FT_TO_M;
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
                261.0, // 261x scale: ~60km base, 38km tall — visible from orbit
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
            let up = ap.ecef.normalize();
            objects[scene_idx].world_pos = ap.ecef + up * 2_000.0;
            objects[scene_idx].rotation = coords::enu_to_ecef_quat(ap.lat_rad, ap.lon_rad);
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

