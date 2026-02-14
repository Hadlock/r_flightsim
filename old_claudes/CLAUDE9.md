# CLAUDE.md — shaderflight: AI Traffic Planes

## Project Context

shaderflight is a wgpu flight simulator with Sobel edge-detection wireframe rendering.
All world math is WGS-84 ECEF with ENU local frames. Physics is 6-DOF RK4 at 120Hz.
See the main CLAUDE.md for full architecture details.

Key files you'll need to reference:
```
coords.rs        – lla_to_ecef, ecef_to_lla, enu_frame_at, ENUFrame
scene.rs         – SceneObject, spawn(), load_aircraft_from_path()
sim.rs           – SimRunner (you won't modify this, but match its patterns)
physics.rs       – AircraftParams, Simulation (reference only — AI planes don't use the full physics)
main.rs          – event loop, where AI traffic gets ticked and rendered
```

Aircraft OBJ: `assets/planes/ki61_hien/model.obj` (wingspan: 12.0m, OBJ span: ~2.2019 units)
FSBLUE clear color: `(0.10, 0.20, 0.30, 1.0)`

---

## Task: AI Traffic Planes

Add 5 AI-controlled Ki-61 aircraft that fly lazy figure-8 patterns between three waypoints
in the SF Bay Area. This is cosmetic window dressing — not a real autopilot. It should be
visually convincing from a distance but the flight model is intentionally crude. The code
should be self-contained and easy to rip out or replace later.

### Files to create

```
src/ai_traffic.rs    – all AI traffic logic (state machine, steering, spawning)
```

Modify:
```
main.rs              – instantiate AiTrafficManager, tick it, update SceneObjects
```

Do NOT modify: `renderer.rs`, shaders, `physics.rs`, `sim.rs`

---

## Waypoints

Three loiter waypoints (the planes circle around these):

```rust
const WAYPOINTS: [(f64, f64); 3] = [
    (37.647939, -122.410925),   // WP0: near SFO, south of city
    (37.792415, -122.297972),   // WP1: east bay, near Emeryville
    (37.818184, -122.484053),   // WP2: Golden Gate area
];
```

Avoidance point (San Bruno Mountain peak):
```rust
const SAN_BRUNO_PEAK: (f64, f64, f64) = (37.685252, -122.434665, 400.0);
// 400m elevation — AI planes must not fly below 450m MSL within 2km horizontal of this point
```

---

## Per-Plane State

Each AI plane has:

```rust
pub struct AiPlane {
    // Identity
    id: usize,

    // Position & motion (ECEF, like everything else)
    pos_ecef: DVec3,
    orientation: DQuat,

    // Flight parameters (randomized at spawn, fixed for lifetime)
    speed_mps: f64,          // ground speed in m/s (picked from 145–350 kts → ~74.6–180.0 m/s)
    altitude_m: f64,         // cruise altitude MSL in meters (300–2400 ft → ~91–732 m)

    // Navigation state machine
    nav_state: NavState,
    current_wp: usize,       // index into WAYPOINTS
    target_wp: usize,        // next waypoint to fly to (when transiting)

    // Loiter state
    loiter_angle: f64,       // current angle around the loiter circle (radians)
    loiter_remaining: f64,   // seconds left to loiter before picking next WP
    loiter_clockwise: bool,  // randomize direction per-plane

    // Steering
    heading: f64,            // current heading in radians (ENU: 0=north, positive=clockwise)
    bank_angle: f64,         // current bank: 0.0 (level) or ±LOITER_BANK (turning)
}

enum NavState {
    Loiter,     // circling around current_wp
    Transit,    // flying straight to target_wp
}
```

---

## Behavior

### Spawn
- Create 5 planes at startup
- Each starts at a random waypoint, at a random point on its loiter circle
- Randomize: speed (uniform 145–350 kts), altitude (uniform 300–2400 ft), loiter direction
- Use a seeded RNG (`rand` crate with `StdRng::seed_from_u64(42 + id)`) for reproducibility

### Loiter (circling)
- Circle radius: ~1500m (gives reasonable visual at various speeds)
- Bank angle: ~20° (standard rate-ish, visually convincing)
- The plane follows a circle centered on the waypoint at its cruise altitude
- Each tick: advance `loiter_angle` based on speed and radius: `dθ = (speed / radius) * dt`
- Loiter duration: random 30–90 seconds per visit
- When loiter_remaining hits 0, pick a new waypoint (different from current) → transition to Transit

### Transit (straight-line)
- Fly level (bank = 0°) directly toward target_wp at cruise speed and altitude
- Heading: great circle bearing from current position to target WP (but a flat ENU bearing is fine at these distances — they're all within ~20km)
- Each tick: move `pos_ecef` along the heading at `speed_mps * dt`
- When within 1500m of target_wp (i.e., reaching the loiter circle), switch to Loiter state
- Smoothly transition heading into the loiter circle tangent — or just snap, it's fine for v1

### San Bruno Avoidance
- Simple: during transit, if the straight-line path passes within 2km horizontal of San Bruno peak AND the plane's altitude is below 450m MSL, temporarily raise altitude to 500m for that segment
- Or even simpler: if any plane's altitude is randomly assigned below 450m, just ensure it's at least 500m when within 2.5km horizontal of the avoidance point. Check each tick.
- Don't overthink this — it's to prevent obvious mountain clipping, not real terrain following

### Heading / Orientation
- Compute heading from velocity direction in ENU
- Build orientation quaternion from heading + bank angle:
  1. Get ENU frame at current position
  2. Nose direction: heading rotated from north in ENU horizontal plane
  3. Apply bank angle: rotate around nose axis
  4. Convert body axes to ECEF → DQuat (same pattern as physics.rs initial conditions)
- The orientation drives the SceneObject's rotation each frame

---

## Integration with main.rs

### Struct

```rust
pub struct AiTrafficManager {
    planes: Vec<AiPlane>,
    scene_object_ids: Vec<usize>,  // indices into the main objects Vec
}
```

### Lifecycle

1. **Startup** (after scene is loaded, before entering Flying state):
   ```rust
   let ai_traffic = AiTrafficManager::new();
   // Spawn 5 SceneObjects using the Ki-61 mesh, append to objects Vec
   // Store the indices so we can update their positions each frame
   for plane in &ai_traffic.planes {
       let obj = scene::load_aircraft_from_path(device, ki61_path, 12.0, next_object_id);
       objects.push(obj);
       // track index
   }
   ```

2. **Each frame** (in the Flying state update loop, after player physics):
   ```rust
   ai_traffic.update(dt);  // advance all AI planes
   // Update SceneObject positions/rotations from AI plane state
   for (plane, &obj_idx) in ai_traffic.planes.iter().zip(&ai_traffic.scene_object_ids) {
       objects[obj_idx].world_pos = plane.pos_ecef;
       objects[obj_idx].rotation = dquat_to_quat(plane.orientation);
   }
   ```

3. **Menu state**: AI traffic is NOT ticked. It only runs during Flying state.

### Object ID allocation
- The main scene already uses IDs. AI planes should use IDs starting from 100 (or whatever is safely above the existing scene objects + airports). The exact scheme is up to you — just don't collide with existing IDs.

---

## Implementation Notes

- **Keep it simple.** This is ~200-300 lines total. No pathfinding, no wind, no aerodynamics.
  The planes are essentially moving points with a heading and a bank angle.
- **ENU is fine for steering math.** All three waypoints are within 20km of each other.
  Flat-earth lateral math in ENU won't introduce meaningful error.
- **Position update:** Each tick, convert current pos to LLA, do steering math in ENU,
  compute new ENU displacement, convert back to ECEF. Or work directly in ECEF if you prefer.
  Either way, altitude is maintained as a fixed MSL value — just set `lla.alt = altitude_m`
  each tick.
- **Don't use the full Simulation/RigidBody system.** That's overkill. These planes just need
  `pos_ecef`, `orientation`, `speed`, `heading`, `bank`. Compute position and orientation
  directly from these each frame.
- **Heading smoothing:** When transitioning from loiter to transit or changing bank, you can
  snap instantly for v1. If it looks janky, add a simple heading rate limit (~3°/sec standard
  rate turn) but don't over-invest here.
- **Rendering:** These are normal SceneObjects. They go through the same Sobel pipeline as
  everything else. They'll appear as white wireframe Ki-61s in the sky. That's the whole point.

---

## Dependencies

```toml
rand = "0.8"    # if not already present — for randomized spawn parameters
```

Use `StdRng` with a fixed seed so the AI traffic is deterministic across runs.

---

## Do NOT

- Modify `renderer.rs` or shaders
- Use the full physics `Simulation` for AI planes
- Add collision detection between AI and player (future work)
- Add ATC or radio calls (future work)  
- Spend time on realistic flight dynamics — these are dots that look like planes
- Use f32 for ECEF positions

## Verification

After implementation, `cargo run --release -- -i` should show:
- 5 additional Ki-61 wireframes visible in the sky around the Bay Area
- They circle lazily at various altitudes and speeds
- Occasionally one transits between waypoints in a straight line
- None clip through San Bruno Mountain
- No performance impact (they're just 5 extra SceneObjects with trivial per-frame math)