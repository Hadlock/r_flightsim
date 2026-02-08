# CLAUDE.md — Ground Contact Model

## Problem
The current ground check in `physics.rs` only clamps altitude and zeros downward velocity. This causes:
1. Aircraft pitches nose up on its own at ~110kt because lift exceeds weight but there's no normal force to prevent rotation
2. Orientation is unconstrained on the ground — the plane can pitch/roll freely while wheels are on the runway
3. No proper weight-on-wheels normal force, just a velocity clamp

## Goal
Replace the crude ground clamp with a spring-damper landing gear model that provides:
- Normal force opposing gravity when on ground (weight on wheels)
- Pitch/roll constraint from gear geometry (3-point contact: nose gear + two mains)
- Proper takeoff rotation: pilot must use elevator at sufficient speed to rotate
- Realistic ground roll friction (rolling + braking)
- Nosewheel steering via rudder input

## Changes — All in `physics.rs`

### 1. Add Landing Gear Geometry to AircraftParams

Add gear contact points in body frame (X=forward, Y=right, Z=down).
Ki-61 is a taildragger (two main gear + tail wheel):

```rust
pub struct GearContact {
    pub pos_body: DVec3,      // attachment point in body frame
    pub spring_k: f64,        // spring constant (N/m)
    pub damping: f64,         // damping coefficient (N·s/m)
    pub rolling_friction: f64, // rolling friction coefficient
    pub braking_friction: f64, // braking friction coefficient
    pub is_steerable: bool,   // does rudder input steer this wheel
}
```

Add to AircraftParams:
```rust
pub gear: Vec<GearContact>,
```

Ki-61 gear positions (approximate, in body frame X=fwd, Y=right, Z=down):
```rust
// Main gear: ~1m behind CG, 2m apart laterally, ~2m below CG
// Tail wheel: ~5m behind CG, centerline, ~1.5m below CG
gear: vec![
    GearContact {  // Left main
        pos_body: DVec3::new(-1.0, -2.0, 2.0),
        spring_k: 50_000.0,
        damping: 10_000.0,
        rolling_friction: 0.03,
        braking_friction: 0.5,
        is_steerable: false,
    },
    GearContact {  // Right main
        pos_body: DVec3::new(-1.0, 2.0, 2.0),
        spring_k: 50_000.0,
        damping: 10_000.0,
        rolling_friction: 0.03,
        braking_friction: 0.5,
        is_steerable: false,
    },
    GearContact {  // Tail wheel
        pos_body: DVec3::new(-5.0, 0.0, 1.5),
        spring_k: 20_000.0,
        damping: 5_000.0,
        rolling_friction: 0.05,
        braking_friction: 0.5,
        is_steerable: true,
    },
],
```

### 2. Add Braking to Controls

```rust
pub struct Controls {
    pub throttle: f64,
    pub elevator: f64,
    pub aileron: f64,
    pub rudder: f64,
    pub brakes: f64,  // 0.0 to 1.0
}
```

Map `KeyCode::KeyB` to brakes (hold = brake). Add to `sim.rs` update_controls:
```rust
c.brakes = if held.contains(&KeyCode::KeyB) { 1.0 } else { 0.0 };
```

### 3. Replace `ground_check()` with `compute_gear_forces()`

Remove the existing `ground_check()` method entirely. Instead, compute gear forces as part of the force/moment calculation so they participate in RK4 integration properly.

**Add this function and call it from `compute_forces_and_moments()`:**

```rust
/// Compute forces and moments from landing gear ground contact.
/// Returns (force_ecef, moment_body) contribution from all gear.
fn compute_gear_forces(
    params: &AircraftParams,
    state: &OdeState,
    controls: &Controls,
) -> (DVec3, DVec3) {
    let q = state.orientation();
    let lla = coords::ecef_to_lla(state.pos);
    let enu = coords::enu_frame_at(lla.lat, lla.lon, state.pos);

    let mut total_force_ecef = DVec3::ZERO;
    let mut total_moment_body = DVec3::ZERO;

    for gear in &params.gear {
        // Gear contact point in ECEF
        let gear_ecef = state.pos + q * gear.pos_body;
        let gear_lla = coords::ecef_to_lla(gear_ecef);

        // Compression: how far below ground the contact point is
        // Positive compression = gear is touching/compressed
        let compression = -gear_lla.alt;

        if compression <= 0.0 {
            continue; // wheel not touching ground
        }

        // Velocity of gear contact point in ECEF
        // v_contact = v_cg + omega_body × r_body (transformed to ECEF)
        let omega_cross_r = state.omega.cross(gear.pos_body);
        let v_contact_ecef = state.vel + q * omega_cross_r;

        // Vertical velocity of contact point (in ENU up direction)
        let v_contact_enu = enu.ecef_to_enu(v_contact_ecef);
        let v_vertical = v_contact_enu.z; // positive = moving up

        // --- Normal force (spring-damper, only pushes up) ---
        let normal_mag = (gear.spring_k * compression - gear.damping * v_vertical).max(0.0);
        let normal_force_ecef = enu.up * normal_mag;

        // --- Friction force (opposes horizontal velocity) ---
        let v_horizontal_enu = DVec3::new(v_contact_enu.x, v_contact_enu.y, 0.0);
        let h_speed = v_horizontal_enu.length();

        let mut friction_force_ecef = DVec3::ZERO;
        if h_speed > 0.01 {
            // Friction coefficient: blend rolling and braking
            let mu = gear.rolling_friction
                + (gear.braking_friction - gear.rolling_friction) * controls.brakes;

            let friction_mag = mu * normal_mag;
            let friction_dir_enu = -v_horizontal_enu / h_speed;

            // Steerable gear: add lateral force from rudder
            // (rudder deflects the wheel, creating a side force)
            let mut friction_enu = friction_dir_enu * friction_mag;
            if gear.is_steerable {
                // Rudder input creates a lateral force proportional to forward speed
                let steer_angle = controls.rudder * 0.3; // max 17° steer
                let body_right_enu = enu.ecef_to_enu(q * DVec3::Y);
                // Lateral force from steering: sideways component
                friction_enu += body_right_enu * steer_angle * normal_mag * 0.3;
            }

            friction_force_ecef = enu.enu_to_ecef(friction_enu);
        }

        // Total force from this gear leg
        let gear_force_ecef = normal_force_ecef + friction_force_ecef;
        total_force_ecef += gear_force_ecef;

        // Moment about CG from this gear leg (in body frame)
        // torque = r × F, where r is gear position relative to CG (body frame)
        // F needs to be in body frame too
        let gear_force_body = q.conjugate() * gear_force_ecef;
        let moment = gear.pos_body.cross(gear_force_body);
        total_moment_body += moment;
    }

    (total_force_ecef, total_moment_body)
}
```

### 4. Integrate Gear Forces into `compute_forces_and_moments()`

In the existing `compute_forces_and_moments()` function, **add gear forces** after the gravity calculation:

```rust
// ... existing code: aero forces, thrust, gravity ...

// Landing gear ground contact
let (gear_force_ecef, gear_moment_body) = compute_gear_forces(params, state, controls);

ForcesAndMoments {
    force_ecef: force_ecef_aero + gravity_ecef + gear_force_ecef,
    moment_body: moment_body + gear_moment_body,
}
```

### 5. Remove `ground_check()` from `Simulation::step()`

The step method should now be simply:
```rust
pub fn step(&mut self, dt: f64) {
    self.integrate_rk4(dt);
    self.aircraft.update_derived();
    self.atmosphere = Atmosphere::at_altitude(self.aircraft.lla.alt.max(0.0));
}
```

No more post-integration clamping. The gear spring forces handle everything through proper physics.

### 6. Safety Clamp (keep as backup)

Add a minimal altitude safety clamp ONLY to prevent numerical explosion if the gear springs can't keep up (shouldn't happen with proper spring constants, but good safety net):

```rust
// At end of step(), after update_derived():
if self.aircraft.lla.alt < -5.0 {
    // Something went very wrong — emergency clamp
    log::warn!("Aircraft below -5m, emergency clamp");
    let clamped = LLA {
        lat: self.aircraft.lla.lat,
        lon: self.aircraft.lla.lon,
        alt: 0.0,
    };
    self.aircraft.pos_ecef = coords::lla_to_ecef(&clamped);
    self.aircraft.vel_ecef = DVec3::ZERO;
    self.aircraft.angular_vel_body = DVec3::ZERO;
    self.aircraft.update_derived();
}
```

### 7. Add `on_ground` flag to RigidBody

Useful for telemetry and future logic:

```rust
pub struct RigidBody {
    // ... existing fields ...
    pub on_ground: bool,  // true if any gear is compressed
}
```

Set it during `update_derived()` or after gear force computation. Simplest approach: check if `lla.alt < 3.0` (approximate gear height). Or better: add a method that checks gear compression:

```rust
impl RigidBody {
    pub fn check_on_ground(&mut self, gear: &[GearContact]) {
        self.on_ground = gear.iter().any(|g| {
            let gear_ecef = self.pos_ecef + self.orientation * g.pos_body;
            let gear_lla = coords::ecef_to_lla(gear_ecef);
            gear_lla.alt < 0.0
        });
    }
}
```

Call after `update_derived()` in `step()`.

### 8. Update Telemetry in `sim.rs`

Add weight-on-wheels and brakes to the telemetry output:

```rust
let wow = if a.on_ground { "GND" } else { "AIR" };
let brk = if self.sim.controls.brakes > 0.0 { "BRK" } else { "   " };

println!(
    "HDG:{:5.1}° PIT:{:+5.1}° BNK:{:+5.1}° | \
     GS:{:5.1}kt VS:{:+6.0}fpm ALT:{:6.0}ft | \
     THR:{:3.0}% {} {} | \
     {:.4}°{} {:.4}°{}",
    hdg, pitch_deg, bank_deg,
    gs_kts, vs_fpm, alt_ft,
    throttle_pct, wow, brk,
    lat.abs(), if lat >= 0.0 { "N" } else { "S" },
    lon.abs(), if lon >= 0.0 { "E" } else { "W" },
);
```

## Expected Behavior After Changes

1. **Stationary on ground**: Gear springs support aircraft weight, nose points slightly up (taildragger attitude ~10° nose up), aircraft sits still
2. **Throttle up, taxi**: Aircraft accelerates forward, stays on ground, rolling friction provides mild deceleration
3. **Approaching rotation speed (~80-90kt)**: Elevator input can raise the nose. Without elevator, aircraft stays on ground — the tail wheel and main gear geometry prevent spontaneous pitch-up
4. **Rotation and liftoff**: Pull back on elevator, nose pitches up, AoA increases, when lift > weight the gear unloads and aircraft flies
5. **Landing**: Descend toward ground, gear springs absorb impact, friction decelerates
6. **Brakes (B key)**: High friction on ground for stopping

## What This Fixes
- No more spontaneous pitch-up at 110kt
- No more unconstrained orientation on ground
- Proper takeoff requires pilot elevator input
- Landing is survivable (spring-damper absorbs impact)
- Taildragger ground attitude (slight nose-up)

## Files Modified
- `physics.rs` — gear model, force integration, remove old ground_check
- `sim.rs` — brake key binding, telemetry update

## Files NOT Modified
- `renderer.rs`, `shaders/*`, `obj_loader.rs`, `coords.rs`, `camera.rs`, `main.rs`, `scene.rs`

## Test
1. `cargo run --release`
2. Aircraft should sit on ground, slight nose-up attitude
3. Hold Shift (throttle up), watch GS increase in telemetry
4. No pitch-up until you press Up arrow at speed
5. At ~90kt, Up arrow rotates nose, aircraft lifts off
6. Press B to brake on ground
7. Hard landing from altitude should bounce, not clip through ground

## Tuning Notes
If the aircraft bounces excessively on the ground, increase `damping` values.
If it sinks through the ground, increase `spring_k` values.
If it takes off too early/late, adjust `cl0` or gear Z positions.
The taildragger should naturally sit with nose up about 8-12° — if not, adjust tail wheel Z position.