use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::physics::{AircraftParams, GearContact};
use glam::DVec3;

#[derive(Deserialize, Debug, Clone)]
pub struct AircraftProfile {
    pub name: String,
    pub manufacturer: String,
    pub country: String,
    pub year: u32,
    pub description: String,
    pub category: String,
    pub model: ModelSpec,
    pub physics: PhysicsSpec,
    pub engines: Vec<EngineSpec>,
    pub gear: Vec<GearSpec>,
    pub stats: std::collections::HashMap<String, String>,

    // Not in YAML - computed after loading
    #[serde(skip)]
    pub dir_path: PathBuf,
    #[serde(skip)]
    pub slug: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ModelSpec {
    pub obj: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PhysicsSpec {
    pub mass: f64,
    pub inertia: [f64; 3],
    pub wing_area: f64,
    pub wing_span: f64,
    pub max_thrust: f64,
    pub cl0: f64,
    pub cl_alpha: f64,
    pub cd0: f64,
    pub cd_alpha_sq: f64,
    pub stall_alpha: f64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct EngineSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub engine_type: String,
    pub thrust: f64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GearSpec {
    pub name: String,
    pub position: [f64; 3],
}

impl AircraftProfile {
    /// Path to the OBJ model file
    pub fn obj_path(&self) -> PathBuf {
        self.dir_path.join(&self.model.obj)
    }

    /// Check if the OBJ model file exists
    pub fn has_model(&self) -> bool {
        self.obj_path().exists()
    }

    /// Convert profile to runtime physics parameters
    pub fn to_aircraft_params(&self) -> AircraftParams {
        let p = &self.physics;
        // Sum engine thrusts for total max_thrust
        let total_thrust: f64 = self.engines.iter().map(|e| e.thrust).sum();
        // Use engine sum if available, otherwise use physics.max_thrust
        let max_thrust = if total_thrust > 0.0 {
            total_thrust
        } else {
            p.max_thrust
        };

        let gear: Vec<GearContact> = self
            .gear
            .iter()
            .map(|g| {
                let is_steerable = g.name.contains("tail") || g.name.contains("nose");
                // Scale spring/damping by aircraft mass
                let mass = p.mass;
                let spring_k = mass * 20.0; // ~20 * mass gives reasonable spring
                let damping = mass * 4.0; // ~4 * mass for damping

                GearContact {
                    pos_body: DVec3::new(g.position[0], g.position[1], g.position[2]),
                    spring_k,
                    damping,
                    rolling_friction: 0.03,
                    braking_friction: 0.5,
                    is_steerable,
                }
            })
            .collect();

        let mean_chord = p.wing_area / p.wing_span;

        AircraftParams {
            mass: p.mass,
            inertia: DVec3::new(p.inertia[0], p.inertia[1], p.inertia[2]),
            wing_area: p.wing_area,
            max_thrust,
            cl0: p.cl0,
            cl_alpha: p.cl_alpha,
            cd0: p.cd0,
            cd_alpha_sq: p.cd_alpha_sq,
            stall_alpha: p.stall_alpha,
            mean_chord,
            wingspan: p.wing_span,
            // Scale control coefficients based on aircraft size
            cm_elevator: 0.4,
            cl_aileron: 0.15,
            cn_rudder: 0.08,
            pitch_damping: -0.08,
            roll_damping: -0.05,
            yaw_damping: -0.04,
            gear,
        }
    }
}

/// Load all aircraft profiles from the given base directory.
/// Scans for `<base_path>/*/profile.yaml` files.
pub fn load_all_profiles(base_path: &Path) -> Vec<AircraftProfile> {
    let mut profiles = Vec::new();

    let entries = match std::fs::read_dir(base_path) {
        Ok(e) => e,
        Err(e) => {
            log::warn!(
                "Could not read aircraft profiles directory {}: {}",
                base_path.display(),
                e
            );
            return profiles;
        }
    };

    let mut dirs: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    dirs.sort_by_key(|e| e.file_name());

    for entry in dirs {
        let dir_path = entry.path();
        let yaml_path = dir_path.join("profile.yaml");

        if !yaml_path.exists() {
            continue;
        }

        let yaml_str = match std::fs::read_to_string(&yaml_path) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Could not read {}: {}", yaml_path.display(), e);
                continue;
            }
        };

        match serde_yaml::from_str::<AircraftProfile>(&yaml_str) {
            Ok(mut profile) => {
                profile.dir_path = dir_path.clone();
                profile.slug = entry.file_name().to_string_lossy().to_string();
                log::info!(
                    "Loaded aircraft profile: {} ({})",
                    profile.name,
                    profile.slug
                );
                profiles.push(profile);
            }
            Err(e) => {
                log::warn!("Could not parse {}: {}", yaml_path.display(), e);
            }
        }
    }

    log::info!("Loaded {} aircraft profiles", profiles.len());
    profiles
}
