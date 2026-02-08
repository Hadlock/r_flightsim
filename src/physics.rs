use glam::{DMat3, DQuat, DVec3};

use crate::coords::{self, ENUFrame, LLA};

// Physics constants
pub const PHYSICS_HZ: f64 = 120.0;
pub const PHYSICS_DT: f64 = 1.0 / PHYSICS_HZ;
const G: f64 = 9.80665;

// --- Atmosphere (ISA standard model) ---

pub struct Atmosphere {
    pub density: f64,       // kg/m³
    pub temperature: f64,   // K
    pub pressure: f64,      // Pa
    pub speed_of_sound: f64, // m/s
}

impl Atmosphere {
    pub fn at_altitude(alt_m: f64) -> Self {
        const T0: f64 = 288.15;    // sea-level temperature (K)
        const P0: f64 = 101_325.0; // sea-level pressure (Pa)
        const L: f64 = -0.0065;    // troposphere lapse rate (K/m)
        const R: f64 = 287.058;    // specific gas constant (J/(kg·K))
        const GAMMA: f64 = 1.4;    // ratio of specific heats

        let alt = alt_m.max(0.0);

        if alt < 11_000.0 {
            let t = T0 + L * alt;
            let p = P0 * (t / T0).powf(-G / (L * R));
            let rho = p / (R * t);
            let a = (GAMMA * R * t).sqrt();
            Self { density: rho, temperature: t, pressure: p, speed_of_sound: a }
        } else {
            // Stratosphere: constant temperature
            let t11 = T0 + L * 11_000.0;
            let p11 = P0 * (t11 / T0).powf(-G / (L * R));
            let t = t11;
            let p = p11 * ((-G / (R * t)) * (alt - 11_000.0)).exp();
            let rho = p / (R * t);
            let a = (GAMMA * R * t).sqrt();
            Self { density: rho, temperature: t, pressure: p, speed_of_sound: a }
        }
    }
}

// --- Aircraft parameters ---

/// Body frame convention (right-handed):
///   X = forward (nose)
///   Y = right (starboard wing)
///   Z = down
/// This is the standard aerospace NED-aligned body frame.
pub struct AircraftParams {
    pub mass: f64,        // kg
    pub inertia: DVec3,   // principal moments (kg·m²): X=roll, Y=pitch, Z=yaw
    pub wing_area: f64,   // m²
    pub max_thrust: f64,  // N
    pub cl0: f64,         // lift coefficient at zero AoA
    pub cl_alpha: f64,    // lift curve slope (per radian)
    pub cd0: f64,         // parasitic drag coefficient
    pub cd_alpha_sq: f64, // induced drag: CD = cd0 + cd_alpha_sq * alpha²
    pub stall_alpha: f64, // stall angle (rad)
    pub mean_chord: f64,  // mean aerodynamic chord (m)
    pub wingspan: f64,    // wingspan (m)
    // Control moment coefficients (positive = intuitive direction)
    pub cm_elevator: f64, // +elevator → positive pitch moment → nose up
    pub cl_aileron: f64,  // +aileron → positive roll moment → right roll
    pub cn_rudder: f64,   // +rudder → positive yaw moment → nose right
    // Damping coefficients (negative for stability)
    pub pitch_damping: f64,
    pub roll_damping: f64,
    pub yaw_damping: f64,
}

impl AircraftParams {
    /// Ki-61 Hien, approximate parameters
    pub fn ki61() -> Self {
        Self {
            mass: 2_630.0,
            // X=roll(fwd), Y=pitch(right), Z=yaw(down)
            inertia: DVec3::new(8_000.0, 20_000.0, 25_000.0),
            wing_area: 20.0,
            max_thrust: 8_500.0,
            cl0: 0.2,
            cl_alpha: 5.0,
            cd0: 0.025,
            cd_alpha_sq: 0.04,
            stall_alpha: 0.28, // ~16 degrees
            mean_chord: 1.67,  // wing_area / wingspan
            wingspan: 12.0,
            cm_elevator: 0.4,
            cl_aileron: 0.15,
            cn_rudder: 0.08,
            pitch_damping: -0.08,
            roll_damping: -0.05,
            yaw_damping: -0.04,
        }
    }
}

// --- Controls ---

pub struct Controls {
    pub throttle: f64, // 0.0 to 1.0
    pub elevator: f64, // -1.0 (nose down) to 1.0 (nose up)
    pub aileron: f64,  // -1.0 (roll left) to 1.0 (roll right)
    pub rudder: f64,   // -1.0 (yaw left) to 1.0 (yaw right)
}

impl Default for Controls {
    fn default() -> Self {
        Self { throttle: 0.0, elevator: 0.0, aileron: 0.0, rudder: 0.0 }
    }
}

// --- Rigid body state ---

pub struct RigidBody {
    // ECEF state (source of truth)
    pub pos_ecef: DVec3,
    pub vel_ecef: DVec3,
    pub orientation: DQuat,       // body frame → ECEF rotation
    pub angular_vel_body: DVec3,  // angular velocity in body frame (rad/s)

    // Derived (recomputed each tick)
    pub lla: LLA,
    pub enu_frame: ENUFrame,
    pub vel_enu: DVec3,
    pub groundspeed: f64,
    pub vertical_speed: f64,
    pub agl: f64,
}

impl RigidBody {
    pub fn update_derived(&mut self) {
        self.lla = coords::ecef_to_lla(self.pos_ecef);
        self.enu_frame = coords::enu_frame_at(self.lla.lat, self.lla.lon, self.pos_ecef);
        self.vel_enu = self.enu_frame.ecef_to_enu(self.vel_ecef);
        self.groundspeed =
            (self.vel_enu.x * self.vel_enu.x + self.vel_enu.y * self.vel_enu.y).sqrt();
        self.vertical_speed = self.vel_enu.z; // ENU z = up
        self.agl = self.lla.alt;
    }
}

// --- RK4 integration types ---

struct OdeState {
    pos: DVec3,
    vel: DVec3,
    quat: [f64; 4], // (x, y, z, w) matching glam
    omega: DVec3,
}

struct OdeDeriv {
    dpos: DVec3,
    dvel: DVec3,
    dquat: [f64; 4],
    domega: DVec3,
}

impl OdeState {
    fn from_body(body: &RigidBody) -> Self {
        Self {
            pos: body.pos_ecef,
            vel: body.vel_ecef,
            quat: [
                body.orientation.x,
                body.orientation.y,
                body.orientation.z,
                body.orientation.w,
            ],
            omega: body.angular_vel_body,
        }
    }

    fn add_scaled(&self, deriv: &OdeDeriv, dt: f64) -> Self {
        Self {
            pos: self.pos + deriv.dpos * dt,
            vel: self.vel + deriv.dvel * dt,
            quat: [
                self.quat[0] + deriv.dquat[0] * dt,
                self.quat[1] + deriv.dquat[1] * dt,
                self.quat[2] + deriv.dquat[2] * dt,
                self.quat[3] + deriv.dquat[3] * dt,
            ],
            omega: self.omega + deriv.domega * dt,
        }
    }

    fn orientation(&self) -> DQuat {
        DQuat::from_xyzw(self.quat[0], self.quat[1], self.quat[2], self.quat[3]).normalize()
    }
}

impl OdeDeriv {
    fn rk4_combine(k1: &Self, k2: &Self, k3: &Self, k4: &Self) -> Self {
        Self {
            dpos: (k1.dpos + k2.dpos * 2.0 + k3.dpos * 2.0 + k4.dpos) / 6.0,
            dvel: (k1.dvel + k2.dvel * 2.0 + k3.dvel * 2.0 + k4.dvel) / 6.0,
            dquat: [
                (k1.dquat[0] + 2.0 * k2.dquat[0] + 2.0 * k3.dquat[0] + k4.dquat[0]) / 6.0,
                (k1.dquat[1] + 2.0 * k2.dquat[1] + 2.0 * k3.dquat[1] + k4.dquat[1]) / 6.0,
                (k1.dquat[2] + 2.0 * k2.dquat[2] + 2.0 * k3.dquat[2] + k4.dquat[2]) / 6.0,
                (k1.dquat[3] + 2.0 * k2.dquat[3] + 2.0 * k3.dquat[3] + k4.dquat[3]) / 6.0,
            ],
            domega: (k1.domega + k2.domega * 2.0 + k3.domega * 2.0 + k4.domega) / 6.0,
        }
    }
}

/// Quaternion kinematic equation: dq/dt = 0.5 * q * ω_body
/// ω_body encoded as pure quaternion (ωx, ωy, ωz, 0).
fn quat_derivative(q: &[f64; 4], omega: DVec3) -> [f64; 4] {
    let (qx, qy, qz, qw) = (q[0], q[1], q[2], q[3]);
    let (wx, wy, wz) = (omega.x, omega.y, omega.z);

    // Hamilton product q * (wx, wy, wz, 0), stored as (x, y, z, w)
    [
        0.5 * (qw * wx + qy * wz - qz * wy),
        0.5 * (qw * wy + qz * wx - qx * wz),
        0.5 * (qw * wz + qx * wy - qy * wx),
        0.5 * (-(qx * wx + qy * wy + qz * wz)),
    ]
}

// --- Force and moment computation ---

struct ForcesAndMoments {
    force_ecef: DVec3,
    moment_body: DVec3,
}

/// Compute all forces and moments at a given state.
/// Body frame: X=forward, Y=right, Z=down (right-handed).
fn compute_forces_and_moments(
    params: &AircraftParams,
    state: &OdeState,
    controls: &Controls,
) -> ForcesAndMoments {
    let q = state.orientation();
    let lla = coords::ecef_to_lla(state.pos);
    let enu = coords::enu_frame_at(lla.lat, lla.lon, state.pos);
    let atmo = Atmosphere::at_altitude(lla.alt.max(0.0));

    // Velocity in body frame (q.conjugate() rotates ECEF→body)
    let vel_body = q.conjugate() * state.vel;
    let airspeed = vel_body.length();

    // Body velocity components: u=forward(X), v=right(Y), w=down(Z)
    let u = vel_body.x;
    let _v = vel_body.y;
    let w = vel_body.z;

    let mut force_body = DVec3::ZERO;
    let mut moment_body = DVec3::ZERO;

    if airspeed > 0.1 {
        // Angle of attack: positive = nose above velocity
        // With Z=down, positive alpha means airflow has +Z (downward) component
        let alpha = w.atan2(u);
        let alpha_clamped = alpha.clamp(-params.stall_alpha, params.stall_alpha);

        let q_bar = 0.5 * atmo.density * airspeed * airspeed;
        let s = params.wing_area;

        // CL clamped to stall range, CD uses full alpha for extra drag past stall
        let cl = params.cl0 + params.cl_alpha * alpha_clamped;
        let cd = params.cd0 + params.cd_alpha_sq * alpha * alpha;

        let lift_mag = q_bar * s * cl;
        let drag_mag = q_bar * s * cd;

        // Lift perpendicular to velocity in XZ (pitch) plane, toward -Z (up)
        let xz_speed = (u * u + w * w).sqrt();
        if xz_speed > 0.01 {
            // Rotate velocity 90° in XZ plane toward -Z: (w, 0, -u) / |xz|
            let lift_dir = DVec3::new(w, 0.0, -u) / xz_speed;
            let drag_dir = -vel_body / airspeed;
            force_body += lift_dir * lift_mag + drag_dir * drag_mag;
        }

        // Control surface moments
        let c = params.mean_chord;
        let b = params.wingspan;

        // Roll (around X): +aileron → right wing down
        moment_body.x += q_bar * s * b * params.cl_aileron * controls.aileron;
        // Pitch (around Y): +elevator → nose up
        moment_body.y += q_bar * s * c * params.cm_elevator * controls.elevator;
        // Yaw (around Z): +rudder → nose right
        moment_body.z += q_bar * s * b * params.cn_rudder * controls.rudder;

        // Damping (opposes angular rate)
        moment_body.x += q_bar * s * b * params.roll_damping * state.omega.x;
        moment_body.y += q_bar * s * c * params.pitch_damping * state.omega.y;
        moment_body.z += q_bar * s * b * params.yaw_damping * state.omega.z;
    }

    // Thrust along body +X (nose)
    let thrust = params.max_thrust * controls.throttle * (atmo.density / 1.225);
    force_body.x += thrust;

    // Convert body forces to ECEF
    let force_ecef_aero = q * force_body;

    // Gravity in ECEF: -g * mass * ellipsoidal_up
    let gravity_ecef = -enu.up * G * params.mass;

    ForcesAndMoments {
        force_ecef: force_ecef_aero + gravity_ecef,
        moment_body,
    }
}

fn compute_derivatives(
    params: &AircraftParams,
    state: &OdeState,
    controls: &Controls,
) -> OdeDeriv {
    let fm = compute_forces_and_moments(params, state, controls);

    let accel = fm.force_ecef / params.mass;

    // Euler's rotation equation: I * dω/dt = M - ω × (I * ω)
    let i = params.inertia;
    let w = state.omega;
    let iw = DVec3::new(i.x * w.x, i.y * w.y, i.z * w.z);
    let gyro = w.cross(iw);
    let domega = DVec3::new(
        (fm.moment_body.x - gyro.x) / i.x,
        (fm.moment_body.y - gyro.y) / i.y,
        (fm.moment_body.z - gyro.z) / i.z,
    );

    let dquat = quat_derivative(&state.quat, state.omega);

    OdeDeriv {
        dpos: state.vel,
        dvel: accel,
        dquat,
        domega,
    }
}

// --- Simulation ---

pub struct Simulation {
    pub aircraft: RigidBody,
    pub params: AircraftParams,
    pub controls: Controls,
    pub atmosphere: Atmosphere,
}

const GROUND_FRICTION: f64 = 0.05;

impl Simulation {
    pub fn new(params: AircraftParams, aircraft: RigidBody) -> Self {
        let atmo = Atmosphere::at_altitude(aircraft.lla.alt.max(0.0));
        Self {
            aircraft,
            params,
            controls: Controls::default(),
            atmosphere: atmo,
        }
    }

    pub fn step(&mut self, dt: f64) {
        self.integrate_rk4(dt);
        self.aircraft.update_derived();
        self.ground_check();
        self.atmosphere = Atmosphere::at_altitude(self.aircraft.lla.alt.max(0.0));
    }

    fn integrate_rk4(&mut self, dt: f64) {
        let s0 = OdeState::from_body(&self.aircraft);

        let k1 = compute_derivatives(&self.params, &s0, &self.controls);
        let s1 = s0.add_scaled(&k1, dt * 0.5);
        let k2 = compute_derivatives(&self.params, &s1, &self.controls);
        let s2 = s0.add_scaled(&k2, dt * 0.5);
        let k3 = compute_derivatives(&self.params, &s2, &self.controls);
        let s3 = s0.add_scaled(&k3, dt);
        let k4 = compute_derivatives(&self.params, &s3, &self.controls);

        let combined = OdeDeriv::rk4_combine(&k1, &k2, &k3, &k4);
        let final_state = s0.add_scaled(&combined, dt);

        self.aircraft.pos_ecef = final_state.pos;
        self.aircraft.vel_ecef = final_state.vel;
        self.aircraft.orientation = final_state.orientation(); // normalizes
        self.aircraft.angular_vel_body = final_state.omega;
    }

    fn ground_check(&mut self) {
        if self.aircraft.lla.alt < 0.0 {
            let clamped = LLA {
                lat: self.aircraft.lla.lat,
                lon: self.aircraft.lla.lon,
                alt: 0.0,
            };
            self.aircraft.pos_ecef = coords::lla_to_ecef(&clamped);
            self.aircraft.lla.alt = 0.0;
            self.aircraft.agl = 0.0;

            let vel_enu = self.aircraft.enu_frame.ecef_to_enu(self.aircraft.vel_ecef);
            if vel_enu.z < 0.0 {
                let mut corrected = DVec3::new(vel_enu.x, vel_enu.y, 0.0);
                corrected *= (1.0 - GROUND_FRICTION * PHYSICS_DT).max(0.0);
                self.aircraft.vel_ecef = self.aircraft.enu_frame.enu_to_ecef(corrected);
                self.aircraft.vel_enu = corrected;
            }

            self.aircraft.angular_vel_body = DVec3::ZERO;
        }
    }
}

// --- Initial conditions ---

/// Create a RigidBody on SFO runway 28L, heading 280° true, stationary.
pub fn create_aircraft_at_sfo() -> RigidBody {
    let lat = 37.613931_f64.to_radians();
    let lon = (-122.358089_f64).to_radians();
    let pos = coords::lla_to_ecef(&LLA { lat, lon, alt: 0.0 });
    let enu = coords::enu_frame_at(lat, lon, pos);

    // Heading 280° clockwise from north
    let hdg = 280.0_f64.to_radians();

    // Body axes in ENU (X=forward, Y=right, Z=down)
    let fwd_enu = DVec3::new(hdg.sin(), hdg.cos(), 0.0);
    let right_enu = DVec3::new(hdg.cos(), -hdg.sin(), 0.0);
    let down_enu = DVec3::new(0.0, 0.0, -1.0);

    // Convert to ECEF
    let fwd_ecef = enu.enu_to_ecef(fwd_enu);
    let right_ecef = enu.enu_to_ecef(right_enu);
    let down_ecef = enu.enu_to_ecef(down_enu);

    // Rotation matrix: columns are body X, Y, Z expressed in ECEF (right-handed)
    let mat = DMat3::from_cols(fwd_ecef, right_ecef, down_ecef);
    let orientation = DQuat::from_mat3(&mat);

    let mut body = RigidBody {
        pos_ecef: pos,
        vel_ecef: DVec3::ZERO,
        orientation,
        angular_vel_body: DVec3::ZERO,
        lla: LLA { lat, lon, alt: 0.0 },
        enu_frame: enu,
        vel_enu: DVec3::ZERO,
        groundspeed: 0.0,
        vertical_speed: 0.0,
        agl: 0.0,
    };
    body.update_derived();
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isa_sea_level() {
        let a = Atmosphere::at_altitude(0.0);
        assert!((a.density - 1.225).abs() < 0.001);
        assert!((a.temperature - 288.15).abs() < 0.01);
        assert!((a.pressure - 101_325.0).abs() < 1.0);
        assert!((a.speed_of_sound - 340.3).abs() < 0.5);
    }

    #[test]
    fn isa_5000m() {
        let a = Atmosphere::at_altitude(5000.0);
        assert!((a.temperature - 255.65).abs() < 0.1);
        assert!((a.density - 0.736).abs() < 0.01);
    }

    #[test]
    fn isa_11km_boundary() {
        let a = Atmosphere::at_altitude(11_000.0);
        assert!((a.temperature - 216.65).abs() < 0.1);
    }

    #[test]
    fn isa_15km_stratosphere() {
        let a = Atmosphere::at_altitude(15_000.0);
        assert!((a.temperature - 216.65).abs() < 0.1);
        let a11 = Atmosphere::at_altitude(11_000.0);
        assert!(a.pressure < a11.pressure);
        assert!(a.density < a11.density);
    }

    #[test]
    fn sfo_initial_conditions() {
        let body = create_aircraft_at_sfo();

        let lat_deg = body.lla.lat.to_degrees();
        let lon_deg = body.lla.lon.to_degrees();
        assert!((lat_deg - 37.613931).abs() < 0.001);
        assert!((lon_deg - (-122.358089)).abs() < 0.001);
        assert!(body.lla.alt.abs() < 1.0);

        // Orientation should be a valid unit quaternion
        let q = body.orientation;
        let len = (q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w).sqrt();
        assert!((len - 1.0).abs() < 1e-10, "quaternion not unit: {len}");

        // Body +X (forward/nose) in ECEF should match heading 280° in ENU
        let nose_ecef = body.orientation * DVec3::X;
        let nose_enu = body.enu_frame.ecef_to_enu(nose_ecef);
        let hdg = 280.0_f64.to_radians();
        assert!((nose_enu.x - hdg.sin()).abs() < 0.01, "nose east: {}", nose_enu.x);
        assert!((nose_enu.y - hdg.cos()).abs() < 0.01, "nose north: {}", nose_enu.y);
        assert!(nose_enu.z.abs() < 0.01, "nose up should be ~0: {}", nose_enu.z);

        // Body +Z (down) in ECEF should point downward in ENU
        let down_ecef = body.orientation * DVec3::Z;
        let down_enu = body.enu_frame.ecef_to_enu(down_ecef);
        assert!(down_enu.z < -0.99, "body Z should be down in ENU: {down_enu:?}");
    }

    #[test]
    fn stationary_on_ground_stays_put() {
        let params = AircraftParams::ki61();
        let body = create_aircraft_at_sfo();
        let initial_pos = body.pos_ecef;
        let mut sim = Simulation::new(params, body);

        for _ in 0..120 {
            sim.step(PHYSICS_DT);
        }

        let drift = (sim.aircraft.pos_ecef - initial_pos).length();
        assert!(drift < 1.0, "aircraft drifted {drift:.3}m after 1s at rest");
        assert!(sim.aircraft.lla.alt.abs() < 1.0, "alt: {}", sim.aircraft.lla.alt);
    }

    #[test]
    fn throttle_accelerates_forward() {
        let params = AircraftParams::ki61();
        let body = create_aircraft_at_sfo();
        let mut sim = Simulation::new(params, body);
        sim.controls.throttle = 1.0;

        for _ in 0..240 {
            sim.step(PHYSICS_DT);
        }

        assert!(
            sim.aircraft.groundspeed > 1.0,
            "groundspeed should increase with throttle: {}",
            sim.aircraft.groundspeed
        );
    }

    #[test]
    fn quat_derivative_pure_rotation() {
        let q = [0.0, 0.0, 0.0, 1.0]; // identity
        let omega = DVec3::new(0.0, 0.0, 1.0); // 1 rad/s around body Z
        let dq = quat_derivative(&q, omega);
        assert!((dq[0] - 0.0).abs() < 1e-12);
        assert!((dq[1] - 0.0).abs() < 1e-12);
        assert!((dq[2] - 0.5).abs() < 1e-12);
        assert!((dq[3] - 0.0).abs() < 1e-12);
    }
}
