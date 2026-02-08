# CLAUDE.md — Physics System + ECEF/ENU Coordinate System

## Context
We have a working wgpu wireframe renderer with Sobel edge detection (see existing src/).
This task adds a physics simulation loop and migrates from flat-earth XYZ to WGS-84 ECEF with ENU local frames.

## Existing Code — Do Not Break
- `main.rs` — winit event loop, wgpu init
- `renderer.rs` — two-pass Sobel edge detection pipeline
- `camera.rs` — fly camera with mouse look
- `scene.rs` — SceneObject with DVec3 world_pos, Quat rotation
- `obj_loader.rs` — OBJ loading with smooth normals
- `shaders/` — geometry.wgsl, edge_detect.wgsl

The renderer, shaders, and obj_loader should not be modified. Changes go into main.rs (event loop), camera.rs (position updates), scene.rs (SceneObject fields), and new files.

## New Files to Create

### `src/coords.rs` — WGS-84 / ECEF / ENU conversions
### `src/physics.rs` — Physics state, 6-DOF rigid body, simulation step
### `src/sim.rs` — Top-level simulation orchestrator (the frame loop)

## WGS-84 / ECEF / ENU — `coords.rs`

Use f64 everywhere for world coordinates. No external crates — implement the math directly.

### Constants
```rust
const WGS84_A: f64 = 6_378_137.0;              // semi-major axis (m)
const WGS84_F: f64 = 1.0 / 298.257_223_563;    // flattening
const WGS84_B: f64 = WGS84_A * (1.0 - WGS84_F); // semi-minor axis
const WGS84_E2: f64 = 1.0 - (WGS84_B * WGS84_B) / (WGS84_A * WGS84_A); // first eccentricity squared
```

### Types
```rust
/// Geodetic position: latitude (rad), longitude (rad), altitude above ellipsoid (m)
pub struct LLA {
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
}

/// East-North-Up rotation matrix at a given lat/lon.
/// Columns are the ENU axes expressed in ECEF.
pub struct ENUFrame {
    pub east: DVec3,
    pub north: DVec3,
    pub up: DVec3,
    pub origin_ecef: DVec3,
}
```

### Required Functions
```rust
/// Geodetic (lat/lon/alt) to ECEF XYZ
pub fn lla_to_ecef(lla: &LLA) -> DVec3

/// ECEF XYZ to geodetic. Use Bowring's iterative method (2-3 iterations sufficient).
pub fn ecef_to_lla(ecef: DVec3) -> LLA

/// Compute the ENU frame at a given lat/lon
pub fn enu_frame_at(lat_rad: f64, lon_rad: f64, origin_ecef: DVec3) -> ENUFrame

/// Convert a vector from ENU to ECEF (rotation only, no translation)
impl ENUFrame {
    pub fn enu_to_ecef(&self, enu: DVec3) -> DVec3
    pub fn ecef_to_enu(&self, ecef: DVec3) -> DVec3
}
```

### ENU Axes from lat/lon (the core math)
```rust
fn enu_axes(lat: f64, lon: f64) -> (DVec3, DVec3, DVec3) {
    let (slat, clat) = lat.sin_cos();
    let (slon, clon) = lon.sin_cos();

    let east  = DVec3::new(-slon,        clon,        0.0);
    let north = DVec3::new(-slat * clon, -slat * slon, clat);
    let up    = DVec3::new( clat * clon,  clat * slon, slat);

    (east, north, up)
}
```

### Unit Tests for coords.rs
Include tests that verify:
- `lla_to_ecef` → `ecef_to_lla` round-trips within 1mm for several known points
- SFO (37.613931, -122.358089, 0.0) produces a sane ECEF position (~4.6M, ~-2.5M, ~3.9M meters)
- ENU axes at equator/prime meridian: east=(0,1,0), north=(0,0,1), up=(1,0,0)
- ENU axes at north pole: up=(0,0,1)

---

## Physics System — `physics.rs`

### Simulation Frame Loop
Every physics tick follows this exact order:

```
┌─────────────────────────────────────────────────────────────┐
│  ORDER OF OPERATIONS (per tick)                              │
├─────────────────────────────────────────────────────────────┤
│  1. Input          — read control surfaces, throttle         │
│  2. Environment    — atmosphere model at current alt         │
│  3. Aerodynamics   — forces/moments from airspeed + AoA     │
│  4. Engine         — thrust from throttle + atmosphere       │
│  5. Flight Control — autopilot / FBW corrections (stub)      │
│  6. Systems        — hydraulics, electrical (stub)           │
│  7. Integration    — RK4 on 6-DOF state                     │
│  8. Ground         — gear contact, friction, normal force    │
│  9. (Render is separate, not in physics tick)                │
└─────────────────────────────────────────────────────────────┘
```

### Physics runs at fixed timestep
- `PHYSICS_HZ: f64 = 120.0` (120 ticks/sec)
- `PHYSICS_DT: f64 = 1.0 / 120.0`
- Use accumulator pattern in the main loop: accumulate wall-clock dt, step physics in fixed increments, render with interpolated state

### Rigid Body State
All state is in ECEF. Physics computations happen in ENU.

```rust
pub struct RigidBody {
    // -- ECEF state (source of truth) --
    pub pos_ecef: DVec3,          // position in ECEF (m)
    pub vel_ecef: DVec3,          // velocity in ECEF (m/s)
    pub orientation: DQuat,       // body frame → ECEF rotation
    pub angular_vel_body: DVec3,  // angular velocity in body frame (rad/s)

    // -- Mass properties --
    pub mass: f64,                // kg
    pub inertia: DVec3,           // principal moments of inertia (kg·m²)

    // -- Derived (recomputed each tick) --
    pub lla: LLA,                 // current geodetic position
    pub enu_frame: ENUFrame,      // ENU at current position
    pub vel_enu: DVec3,           // velocity in ENU
    pub groundspeed: f64,         // horizontal speed (m/s)
    pub vertical_speed: f64,      // climb rate (m/s)
    pub agl: f64,                 // altitude above ground (m) — just alt for now (flat terrain at 0)
}
```

### Body Frame Convention
- X = right (starboard wing)
- Y = up (through canopy)
- Z = forward (out the nose) — **this matters, be consistent**

This means thrust acts along +Z body, lift along +Y body, gravity is -up in ENU converted to body frame.

### Per-Tick Flow (implement as methods on a Simulation struct)

```rust
pub struct Simulation {
    pub aircraft: RigidBody,
    pub controls: Controls,
    pub atmosphere: Atmosphere,
}

pub struct Controls {
    pub throttle: f64,      // 0.0 to 1.0
    pub elevator: f64,      // -1.0 to 1.0 (nose down to nose up)
    pub aileron: f64,       // -1.0 to 1.0 (roll left to roll right)
    pub rudder: f64,        // -1.0 to 1.0 (yaw left to yaw right)
}
```

#### Step 1: Input
Read `Controls` from keyboard state. Map:
- Up/Down arrows → elevator
- Left/Right arrows → aileron
- Z/X → rudder
- Shift/Ctrl or +/- → throttle increment

#### Step 2: Environment (stub for now)
```rust
pub struct Atmosphere {
    pub density: f64,       // kg/m³ (sea level default: 1.225)
    pub temperature: f64,   // K
    pub pressure: f64,      // Pa
    pub speed_of_sound: f64, // m/s
}
impl Atmosphere {
    /// ISA standard atmosphere from altitude
    pub fn at_altitude(alt_m: f64) -> Self { /* ISA model */ }
}
```

#### Step 3: Aerodynamics (simplified)
Compute forces and moments in body frame from airspeed vector, AoA, sideslip.
For V1, use simple linear coefficients:
```
lift = 0.5 * rho * V² * S * CL(alpha)
drag = 0.5 * rho * V² * S * CD(alpha)
```
Where `CL(alpha) = CL0 + CL_alpha * alpha` clamped to stall.
Wing area S and coefficients should be configurable per aircraft.

#### Step 4: Engine (simplified)
```
thrust = max_thrust * throttle * (density / 1.225)
```
Acts along body +Z axis.

#### Step 5-6: Stubs
```rust
fn flight_control_system(&self, _controls: &Controls) -> Controls { /* passthrough */ }
fn systems_update(&mut self) { /* no-op */ }
```

#### Step 7: Integration — RK4
Integrate the 6-DOF state using RK4. State vector:
- position (3) — integrated from velocity
- velocity (3) — integrated from forces/mass
- orientation (4, quaternion) — integrated from angular velocity
- angular velocity (3) — integrated from moments/inertia

**Critical: normalize the quaternion after each RK4 step.**

The forces for RK4 are computed in ENU, then the integration updates ECEF state:
1. Compute all forces in body frame
2. Rotate to ENU using orientation
3. Add gravity in ENU: `(0, 0, -9.80665 * mass)` — that's (E, N, U) so gravity is -U
4. Convert total force from ENU to ECEF for integration
5. Integrate pos_ecef and vel_ecef in ECEF
6. Integrate orientation quaternion (body-frame angular vel)
7. Recompute derived quantities (lla, enu_frame, etc.)

#### Step 8: Ground Interaction
Simple for now:
- If `lla.alt < 0.0`, clamp to 0.0
- Zero out downward velocity component (in ENU up direction)
- Apply friction to horizontal velocity when on ground
- Zero out angular velocities when on ground (crude but functional)

### Gravity Direction
Gravity = `-enu_frame.up * 9.80665` in ECEF. This is the ellipsoidal normal, which is correct for a flight sim (true gravity including geoid undulation is overkill).

---

## Integrating with main.rs

### Fixed Timestep with Interpolation
```rust
// In the main loop:
let mut accumulator: f64 = 0.0;
let mut prev_state: InterpolationState = ...;
let mut curr_state: InterpolationState = ...;

// Each frame:
accumulator += dt;
while accumulator >= PHYSICS_DT {
    prev_state = curr_state.clone();
    simulation.step(PHYSICS_DT);
    curr_state = InterpolationState::from(&simulation);
    accumulator -= PHYSICS_DT;
}
let alpha = accumulator / PHYSICS_DT;
let render_state = InterpolationState::lerp(&prev_state, &curr_state, alpha);
```

### InterpolationState
```rust
struct InterpolationState {
    pos_ecef: DVec3,
    orientation: DQuat,
}
impl InterpolationState {
    fn lerp(a: &Self, b: &Self, t: f64) -> Self {
        Self {
            pos_ecef: a.pos_ecef.lerp(b.pos_ecef, t),
            orientation: a.orientation.slerp(b.orientation, t),
        }
    }
}
```

### Camera follows aircraft
- Camera position = aircraft pos_ecef + offset in body frame (e.g., pilot eye position)
- Camera orientation = aircraft orientation (for now — cockpit view)
- The existing camera-relative rendering in renderer.rs still works: subtract camera ECEF pos from each object's ECEF pos, cast to f32

### SceneObject changes
Update `scene.rs` so the aircraft SceneObject reads its `world_pos` and `rotation` from the `RigidBody` each frame. The other static objects keep their fixed positions. Convert `DQuat` orientation to `Quat` for the model matrix.

---

## Initial Conditions — SFO Runway 28L

Place the aircraft at:
- Lat: 37.613931° N
- Lon: -122.358089° W  
- Alt: 0.0 m (on ellipsoid surface)
- Heading: 280° true (runway 28L heading)
- Speed: 0 m/s (starting stationary)

```rust
let lat = 37.613931_f64.to_radians();
let lon = (-122.358089_f64).to_radians();
let pos = lla_to_ecef(&LLA { lat, lon, alt: 0.0 });
let enu = enu_frame_at(lat, lon, pos);

// Heading 280° = 280° clockwise from north
let hdg = 280.0_f64.to_radians();
// Nose direction in ENU: north rotated by heading
let nose_enu = DVec3::new(hdg.sin(), hdg.cos(), 0.0); // (E, N, U)
let right_enu = DVec3::new(hdg.cos(), -hdg.sin(), 0.0);
let up_enu = DVec3::new(0.0, 0.0, 1.0);

// Convert body axes to ECEF to build orientation quaternion
let nose_ecef = enu.enu_to_ecef(nose_enu);
let right_ecef = enu.enu_to_ecef(right_enu);
let up_ecef = enu.enu_to_ecef(up_enu);
// Build rotation matrix [right, up, nose] (body X, Y, Z) in ECEF columns → DQuat
```

---

## Aircraft Parameters (Ki-61 Hien, approximate)

```rust
mass: 2_630.0,                    // kg (empty) — use for now
wing_area: 20.0,                  // m²
max_thrust: 8_500.0,              // N (~1,175 HP)
inertia: DVec3::new(8_000.0, 25_000.0, 20_000.0), // rough estimates
cl0: 0.2,
cl_alpha: 5.0,                    // per radian
cd0: 0.025,
cd_alpha_sq: 0.04,                // CD = cd0 + cd_alpha_sq * alpha²
stall_alpha: 0.28,                // ~16 degrees
```

---

## Style / Conventions
- All world positions: `f64` / `DVec3` / `DQuat`
- All GPU data: `f32` / `Vec3` / `Quat` (cast at render boundary only)
- Angles internally in radians, display in degrees
- No `unwrap()` on file I/O — use `expect()` with context
- Comments explaining non-obvious physics
- `cargo clippy` clean, no warnings

## Build & Run
```bash
cargo run --release
```
Arrow keys fly the plane, throttle with Shift/Ctrl, mouse look if cursor grabbed.

## Do NOT
- Modify renderer.rs or shaders — rendering pipeline is done
- Use any physics crate (nalgebra, rapier, etc.) — we implement our own
- Use f32 for world state
- Skip RK4 (no Euler integration — it will diverge)
- Forget to normalize quaternions after integration