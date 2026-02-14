use glam::{DMat3, DQuat, DVec3};
use rand::prelude::*;
use rand::rngs::StdRng;

use crate::coords::{self, LLA};

// --- Constants ---

const WAYPOINTS: [(f64, f64); 3] = [
    (37.647939, -122.410925), // WP0: near SFO
    (37.792415, -122.297972), // WP1: east bay, Emeryville
    (37.818184, -122.484053), // WP2: Golden Gate
];

const SAN_BRUNO_PEAK: (f64, f64, f64) = (37.685252, -122.434665, 400.0);
const SAN_BRUNO_AVOID_RADIUS: f64 = 2500.0;
const SAN_BRUNO_MIN_ALT: f64 = 450.0;
const SAN_BRUNO_SAFE_ALT: f64 = 500.0;

const NUM_PLANES: usize = 7;
const LOITER_RADIUS: f64 = 1500.0;
const LOITER_BANK_RAD: f64 = 20.0 * std::f64::consts::PI / 180.0;

const SPEED_MIN_MPS: f64 = 74.6;  // 145 kts
const SPEED_MAX_MPS: f64 = 180.0; // 350 kts
const ALT_MIN_M: f64 = 91.0;     // 300 ft
const ALT_MAX_M: f64 = 732.0;    // 2400 ft
const LOITER_MIN_SEC: f64 = 30.0;
const LOITER_MAX_SEC: f64 = 90.0;

// --- Types ---

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NavState {
    Loiter,
    Transit,
}

pub struct AiPlane {
    pub pos_ecef: DVec3,
    pub orientation: DQuat,

    speed_mps: f64,
    altitude_m: f64,

    nav_state: NavState,
    current_wp: usize,
    target_wp: usize,

    loiter_angle: f64,
    loiter_remaining: f64,
    loiter_clockwise: bool,

    heading: f64,
    bank_angle: f64,

    rng: StdRng,
}

pub struct AiTrafficManager {
    planes: Vec<AiPlane>,
    scene_indices: Vec<usize>,
    wp_ecef: [DVec3; 3],
    san_bruno_ecef: DVec3,
}

// --- Orientation helper ---

/// Build a DQuat from heading (0=north, CW positive) and bank angle in ENU frame.
fn compute_orientation(lla: &LLA, heading: f64, bank: f64) -> DQuat {
    let enu = coords::enu_frame_at(lla.lat, lla.lon, DVec3::ZERO);

    // Body axes in ENU (level flight)
    let nose_enu = DVec3::new(heading.sin(), heading.cos(), 0.0);
    let right_enu = DVec3::new(heading.cos(), -heading.sin(), 0.0);
    let up_enu = DVec3::Z;

    // Apply bank: positive = right wing down
    let right_banked = right_enu * bank.cos() - up_enu * bank.sin();
    let down_banked = -(right_enu * bank.sin() + up_enu * bank.cos());

    // Convert body axes (X=fwd, Y=right, Z=down) to ECEF
    let body_x = enu.enu_to_ecef(nose_enu);
    let body_y = enu.enu_to_ecef(right_banked);
    let body_z = enu.enu_to_ecef(down_banked);

    DQuat::from_mat3(&DMat3::from_cols(body_x, body_y, body_z))
}

// --- AiPlane ---

impl AiPlane {
    fn new(id: usize, wp_ecef: &[DVec3; 3]) -> Self {
        let mut rng = StdRng::seed_from_u64(42 + id as u64);

        let speed_mps = rng.gen_range(SPEED_MIN_MPS..SPEED_MAX_MPS);
        let altitude_m = rng.gen_range(ALT_MIN_M..ALT_MAX_M);
        let loiter_clockwise: bool = rng.gen_bool(0.5);
        let current_wp = rng.gen_range(0..3usize);
        let loiter_angle: f64 = rng.gen_range(0.0..std::f64::consts::TAU);
        let loiter_remaining = rng.gen_range(LOITER_MIN_SEC..LOITER_MAX_SEC);

        // Position on loiter circle
        let wp_lla = coords::ecef_to_lla(wp_ecef[current_wp]);
        let enu = coords::enu_frame_at(wp_lla.lat, wp_lla.lon, wp_ecef[current_wp]);
        let offset_ecef = enu.enu_to_ecef(DVec3::new(
            LOITER_RADIUS * loiter_angle.cos(),
            LOITER_RADIUS * loiter_angle.sin(),
            0.0,
        ));
        let mut lla = coords::ecef_to_lla(wp_ecef[current_wp] + offset_ecef);
        lla.alt = altitude_m;
        let pos_ecef = coords::lla_to_ecef(&lla);

        // Heading from circle tangent
        let heading = if loiter_clockwise {
            loiter_angle.sin().atan2(-loiter_angle.cos())
        } else {
            (-loiter_angle.sin()).atan2(loiter_angle.cos())
        };

        let bank_angle = if loiter_clockwise {
            LOITER_BANK_RAD
        } else {
            -LOITER_BANK_RAD
        };

        let lla_now = coords::ecef_to_lla(pos_ecef);
        let orientation = compute_orientation(&lla_now, heading, bank_angle);

        AiPlane {
            pos_ecef,
            orientation,
            speed_mps,
            altitude_m,
            nav_state: NavState::Loiter,
            current_wp,
            target_wp: current_wp,
            loiter_angle,
            loiter_remaining,
            loiter_clockwise,
            heading,
            bank_angle,
            rng,
        }
    }

    fn update(&mut self, dt: f64, wp_ecef: &[DVec3; 3], san_bruno_ecef: DVec3) {
        match self.nav_state {
            NavState::Loiter => self.update_loiter(dt, wp_ecef),
            NavState::Transit => self.update_transit(dt, wp_ecef),
        }
        self.apply_san_bruno_avoidance(san_bruno_ecef);

        let lla = coords::ecef_to_lla(self.pos_ecef);
        self.orientation = compute_orientation(&lla, self.heading, self.bank_angle);
    }

    fn update_loiter(&mut self, dt: f64, wp_ecef: &[DVec3; 3]) {
        let omega = self.speed_mps / LOITER_RADIUS;
        let d_angle = if self.loiter_clockwise {
            -omega * dt
        } else {
            omega * dt
        };
        self.loiter_angle += d_angle;

        // Position on circle
        let wp_lla = coords::ecef_to_lla(wp_ecef[self.current_wp]);
        let enu = coords::enu_frame_at(wp_lla.lat, wp_lla.lon, wp_ecef[self.current_wp]);
        let offset = enu.enu_to_ecef(DVec3::new(
            LOITER_RADIUS * self.loiter_angle.cos(),
            LOITER_RADIUS * self.loiter_angle.sin(),
            0.0,
        ));
        let mut lla = coords::ecef_to_lla(wp_ecef[self.current_wp] + offset);
        lla.alt = self.altitude_m;
        self.pos_ecef = coords::lla_to_ecef(&lla);

        // Heading from tangent
        self.heading = if self.loiter_clockwise {
            self.loiter_angle.sin().atan2(-self.loiter_angle.cos())
        } else {
            (-self.loiter_angle.sin()).atan2(self.loiter_angle.cos())
        };

        self.bank_angle = if self.loiter_clockwise {
            LOITER_BANK_RAD
        } else {
            -LOITER_BANK_RAD
        };

        // Timer
        self.loiter_remaining -= dt;
        if self.loiter_remaining <= 0.0 {
            // Pick a different waypoint
            let mut next = self.rng.gen_range(0..2usize);
            if next >= self.current_wp {
                next += 1;
            }
            self.target_wp = next;
            self.nav_state = NavState::Transit;
            self.bank_angle = 0.0;
        }
    }

    fn update_transit(&mut self, dt: f64, wp_ecef: &[DVec3; 3]) {
        let lla = coords::ecef_to_lla(self.pos_ecef);
        let enu = coords::enu_frame_at(lla.lat, lla.lon, self.pos_ecef);

        // Bearing to target
        let delta_ecef = wp_ecef[self.target_wp] - self.pos_ecef;
        let delta_enu = enu.ecef_to_enu(delta_ecef);
        self.heading = delta_enu.x.atan2(delta_enu.y); // atan2(east, north)
        self.bank_angle = 0.0;

        // Move along heading
        let disp = enu.enu_to_ecef(DVec3::new(
            self.heading.sin() * self.speed_mps * dt,
            self.heading.cos() * self.speed_mps * dt,
            0.0,
        ));
        let mut new_lla = coords::ecef_to_lla(self.pos_ecef + disp);
        new_lla.alt = self.altitude_m;
        self.pos_ecef = coords::lla_to_ecef(&new_lla);

        // Check arrival at loiter circle
        let horiz_dist = (delta_enu.x * delta_enu.x + delta_enu.y * delta_enu.y).sqrt();
        if horiz_dist < LOITER_RADIUS {
            self.current_wp = self.target_wp;
            self.nav_state = NavState::Loiter;
            self.loiter_remaining = self.rng.gen_range(LOITER_MIN_SEC..LOITER_MAX_SEC);

            // Set loiter angle from current position relative to waypoint
            let wp_lla = coords::ecef_to_lla(wp_ecef[self.current_wp]);
            let wp_enu =
                coords::enu_frame_at(wp_lla.lat, wp_lla.lon, wp_ecef[self.current_wp]);
            let rel_enu = wp_enu.ecef_to_enu(self.pos_ecef - wp_ecef[self.current_wp]);
            self.loiter_angle = rel_enu.y.atan2(rel_enu.x); // atan2(north, east) = theta

            self.bank_angle = if self.loiter_clockwise {
                LOITER_BANK_RAD
            } else {
                -LOITER_BANK_RAD
            };
        }
    }

    fn apply_san_bruno_avoidance(&mut self, san_bruno_ecef: DVec3) {
        if self.altitude_m >= SAN_BRUNO_MIN_ALT {
            return;
        }
        let sb_lla = coords::ecef_to_lla(san_bruno_ecef);
        let enu = coords::enu_frame_at(sb_lla.lat, sb_lla.lon, san_bruno_ecef);
        let delta_enu = enu.ecef_to_enu(self.pos_ecef - san_bruno_ecef);
        let horiz_dist = (delta_enu.x * delta_enu.x + delta_enu.y * delta_enu.y).sqrt();

        if horiz_dist < SAN_BRUNO_AVOID_RADIUS {
            let mut lla = coords::ecef_to_lla(self.pos_ecef);
            lla.alt = SAN_BRUNO_SAFE_ALT;
            self.pos_ecef = coords::lla_to_ecef(&lla);
        }
    }

    // --- Public accessors for ATC system ---

    /// Altitude in feet MSL.
    pub fn altitude_ft(&self) -> f64 {
        self.altitude_m * 3.28084
    }

    /// Speed in knots.
    pub fn speed_kts(&self) -> f64 {
        self.speed_mps * 1.94384
    }

    /// Heading in degrees (0=north, CW positive).
    pub fn heading_deg(&self) -> f64 {
        self.heading.to_degrees().rem_euclid(360.0)
    }

    /// Current navigation state.
    pub fn nav_state(&self) -> NavState {
        self.nav_state
    }

    /// Current waypoint index (0-2).
    pub fn current_waypoint(&self) -> usize {
        self.current_wp
    }

    /// Target waypoint index (0-2).
    pub fn target_waypoint(&self) -> usize {
        self.target_wp
    }
}

// --- AiTrafficManager ---

impl AiTrafficManager {
    pub fn new() -> Self {
        let wp_ecef: [DVec3; 3] = std::array::from_fn(|i| {
            let (lat_deg, lon_deg) = WAYPOINTS[i];
            coords::lla_to_ecef(&LLA {
                lat: lat_deg.to_radians(),
                lon: lon_deg.to_radians(),
                alt: 0.0,
            })
        });

        let san_bruno_ecef = coords::lla_to_ecef(&LLA {
            lat: SAN_BRUNO_PEAK.0.to_radians(),
            lon: SAN_BRUNO_PEAK.1.to_radians(),
            alt: SAN_BRUNO_PEAK.2,
        });

        let planes: Vec<AiPlane> = (0..NUM_PLANES)
            .map(|id| AiPlane::new(id, &wp_ecef))
            .collect();

        log::info!("[ai_traffic] Spawned {} AI planes", planes.len());
        for (i, p) in planes.iter().enumerate() {
            log::info!(
                "  AI#{}: alt={:.0}m speed={:.0}m/s ({:.0}kts) wp={} {}",
                i,
                p.altitude_m,
                p.speed_mps,
                p.speed_mps * 1.94384,
                p.current_wp,
                if p.loiter_clockwise { "CW" } else { "CCW" },
            );
        }

        AiTrafficManager {
            planes,
            scene_indices: Vec::new(),
            wp_ecef,
            san_bruno_ecef,
        }
    }

    pub fn plane_count(&self) -> usize {
        self.planes.len()
    }

    pub fn set_scene_indices(&mut self, indices: Vec<usize>) {
        self.scene_indices = indices;
    }

    pub fn scene_indices(&self) -> &[usize] {
        &self.scene_indices
    }

    pub fn planes(&self) -> &[AiPlane] {
        &self.planes
    }

    pub fn update(&mut self, dt: f64) {
        let dt = dt.min(0.1); // cap to prevent huge jumps
        let wp = self.wp_ecef;
        let sb = self.san_bruno_ecef;
        for plane in &mut self.planes {
            plane.update(dt, &wp, sb);
        }
    }
}
