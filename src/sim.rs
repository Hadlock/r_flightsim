use std::collections::HashSet;

use glam::{DMat4, DQuat, DVec3, Mat4, Quat};
use winit::keyboard::KeyCode;

use crate::physics::{Simulation, PHYSICS_DT};

/// Pilot eye offset in body frame (X=forward, Y=right, Z=down).
/// Roughly at cockpit position: 2m behind nose tip, 1m above centerline.
const PILOT_EYE_BODY: DVec3 = DVec3::new(2.0, 0.0, -1.0);

/// Throttle change rate per second when key is held
const THROTTLE_RATE: f64 = 0.5;

// --- Interpolation ---

#[derive(Clone)]
pub struct InterpolationState {
    pub pos_ecef: DVec3,
    pub orientation: DQuat,
}

impl InterpolationState {
    pub fn from_sim(sim: &Simulation) -> Self {
        Self {
            pos_ecef: sim.aircraft.pos_ecef,
            orientation: sim.aircraft.orientation,
        }
    }

    pub fn lerp(a: &Self, b: &Self, t: f64) -> Self {
        Self {
            pos_ecef: a.pos_ecef.lerp(b.pos_ecef, t),
            orientation: a.orientation.slerp(b.orientation, t),
        }
    }
}

// --- SimRunner: fixed-timestep accumulator ---

pub struct SimRunner {
    pub sim: Simulation,
    accumulator: f64,
    prev_state: InterpolationState,
    curr_state: InterpolationState,
    held_keys: HashSet<KeyCode>,
}

impl SimRunner {
    pub fn new(sim: Simulation) -> Self {
        let state = InterpolationState::from_sim(&sim);
        Self {
            sim,
            accumulator: 0.0,
            prev_state: state.clone(),
            curr_state: state,
            held_keys: HashSet::new(),
        }
    }

    pub fn key_down(&mut self, key: KeyCode) {
        self.held_keys.insert(key);
    }

    pub fn key_up(&mut self, key: KeyCode) {
        self.held_keys.remove(&key);
    }

    /// Update controls from currently held keys and advance physics.
    pub fn update(&mut self, dt: f64) {
        self.update_controls(dt);

        // Accumulate wall-clock time and step physics at fixed rate
        self.accumulator += dt;
        // Cap to prevent spiral of death
        if self.accumulator > 0.1 {
            self.accumulator = 0.1;
        }
        while self.accumulator >= PHYSICS_DT {
            self.prev_state = self.curr_state.clone();
            self.sim.step(PHYSICS_DT);
            self.curr_state = InterpolationState::from_sim(&self.sim);
            self.accumulator -= PHYSICS_DT;
        }
    }

    /// Get interpolated state for smooth rendering between physics steps.
    pub fn render_state(&self) -> InterpolationState {
        let alpha = self.accumulator / PHYSICS_DT;
        InterpolationState::lerp(&self.prev_state, &self.curr_state, alpha)
    }

    /// Camera ECEF position (pilot eye in world space).
    pub fn camera_position(&self, render_state: &InterpolationState) -> DVec3 {
        render_state.pos_ecef + render_state.orientation * PILOT_EYE_BODY
    }

    fn update_controls(&mut self, dt: f64) {
        let held = &self.held_keys;
        let c = &mut self.sim.controls;

        // Elevator: Up arrow = nose up (+1), Down arrow = nose down (-1)
        c.elevator = key_axis(held, KeyCode::ArrowUp, KeyCode::ArrowDown);

        // Aileron: Right arrow = roll right (+1), Left arrow = roll left (-1)
        c.aileron = key_axis(held, KeyCode::ArrowRight, KeyCode::ArrowLeft);

        // Rudder: X = yaw right (+1), Z = yaw left (-1)
        c.rudder = key_axis(held, KeyCode::KeyX, KeyCode::KeyZ);

        // Throttle: incremental with Equal(+)/Minus(-) or Shift(+)/Ctrl(-)
        let throttle_up =
            held.contains(&KeyCode::Equal) || held.contains(&KeyCode::ShiftLeft);
        let throttle_down =
            held.contains(&KeyCode::Minus) || held.contains(&KeyCode::ControlLeft);
        if throttle_up {
            c.throttle = (c.throttle + THROTTLE_RATE * dt).min(1.0);
        }
        if throttle_down {
            c.throttle = (c.throttle - THROTTLE_RATE * dt).max(0.0);
        }
    }
}

/// Returns +1.0 if pos_key held, -1.0 if neg_key held, 0.0 otherwise.
fn key_axis(held: &HashSet<KeyCode>, pos_key: KeyCode, neg_key: KeyCode) -> f64 {
    let pos = held.contains(&pos_key) as i32;
    let neg = held.contains(&neg_key) as i32;
    (pos - neg) as f64
}

// --- View matrix from aircraft orientation ---

/// Compute the camera-at-origin view matrix from aircraft orientation.
/// Body frame: X=forward, Y=right, Z=down.
/// View convention: camera looks along -Z, with +Y up.
pub fn aircraft_view_matrix(orientation: DQuat) -> Mat4 {
    let forward = orientation * DVec3::X;
    // Body -Z = up (since body Z = down)
    let up = orientation * DVec3::new(0.0, 0.0, -1.0);
    let view = DMat4::look_at_rh(DVec3::ZERO, forward, up);
    let cols = view.to_cols_array();
    Mat4::from_cols_array(&cols.map(|v| v as f32))
}

/// Convert DQuat (f64) to Quat (f32) for GPU/scene objects.
pub fn dquat_to_quat(dq: DQuat) -> Quat {
    Quat::from_xyzw(dq.x as f32, dq.y as f32, dq.z as f32, dq.w as f32)
}
