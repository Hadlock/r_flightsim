# CLAUDE.md — shaderflight

## Project Overview

A WGPU-based flight simulator rendering engine with a Ki-61 physics model,
edge-detection shader pipeline, and procedural airport generation. Aircraft
starts on SFO runway 28L. Coordinate system is WGS-84 ECEF throughout, with
ENU local frames for placement.

## Architecture

```
main.rs          – winit event loop, wgpu init, glues everything together
renderer.rs      – 2-pass renderer: geometry → G-buffer, then edge detection
scene.rs         – SceneObject type, OBJ loading with # origin: metadata
camera.rs        – Pilot camera (position set by sim, mouse controls head look)
coords.rs        – WGS-84 ECEF ↔ LLA ↔ ENU conversions
obj_loader.rs    – OBJ mesh loading via tobj, smooth normal computation
physics.rs       – 6-DOF rigid body, RK4 integrator, ISA atmosphere, gear contact
sim.rs           – Fixed-timestep accumulator, input handling, telemetry
airport_gen.rs   – Procedural airport geometry from JSON (NEW)
```

## Body Frame Convention

Right-handed: **X = forward (nose), Y = right (starboard), Z = down**.
This is standard aerospace NED-aligned body frame.

## airport_gen.rs — Procedural Airport Generation

### Data Source

Reads `assets/airports/airports_all.json` — a JSON array of airport objects.
Each airport has `ident`, `type`, `latitude`, `longitude`, `elevation_ft`, and
a `runways` array with `length_ft`, `width_ft`, `le_heading_degT`, `le_ident`, etc.

### Load Radius

Only airports within 200km (`LOAD_RADIUS_M`) of the aircraft starting position
are generated, keeping scene object count manageable (~300 for the SFO area).

### Heading Inference

If `le_heading_degT` is null (common — ~32k of ~47k runways), the heading is
inferred from `le_ident` (e.g., "09" → 90°, "27L" → 270°). Runways with
neither are skipped.

### What It Generates

For every non-heliport airport:

1. **Runways** — flat rectangles (0.3m thick) at field elevation, oriented by
   `le_heading_degT`. All runways for one airport are merged into a single
   `SceneObject` (`{IDENT}_runways`).

2. **Buildings** — merged into a single `SceneObject` (`{IDENT}_buildings`):
   - **ATC tower**: 10×10×30m (always 1)
   - **Hangar type 1**: 45×80×20m
   - **Hangar type 2**: 40×70×15m
   - **Admin building**: 33×33×10m
   - **Aux buildings**: 1–32 procedurally sized (10–35m × 10–35m × 6–12m)

   Counts scale by airport size class:
   | Size            | Hangar 1 | Hangar 2 | Admin |
   |-----------------|----------|----------|-------|
   | small_airport   | 1        | 1        | 1     |
   | medium_airport  | 2        | 2        | 1     |
   | large_airport   | 6        | 4        | 8     |

   Aux building count is `(hash(ident) >> 8) % 32 + 1`.

### Placement Algorithm

- Buildings are placed on one side of the longest runway (side chosen by
  hash parity of ident).
- All buildings are aligned with the primary runway heading.
- Placement uses a retry loop (up to 40 attempts per building) with
  deterministic pseudo-random offsets seeded by `hash(ident, building_index)`.
- **Collision avoidance** uses 2D Separating Axis Theorem (SAT) on oriented
  bounding boxes. Each building footprint includes 2m padding. Buildings must
  not overlap each other or any runway footprint.
- ATC tower overlap with other buildings is acceptable (it's narrow).

### Coordinate Pipeline

Geometry is created in a local ENU frame (X=east, Y=north, Z=up) centred at
the airport lat/lon/elevation. The `SceneObject.rotation` is set to the
ENU→ECEF quaternion so the renderer places it correctly on the globe.

### Determinism

All procedural choices (side selection, building positions, aux building
dimensions) are seeded from `std::hash::DefaultHasher` on the airport ident
string. The same JSON input always produces the same geometry.

### Integration

In `main.rs`, after `scene::load_scene()` loads OBJ-based landmarks:

```rust
let ref_ecef = sim_runner.render_state().pos_ecef;
let (airport_objects, _next_id) =
    airport_gen::generate_airports(&device, airport_json, next_id, ref_ecef);
objects.extend(airport_objects);
```

Aircraft object is pushed last so `aircraft_idx` stays correct.

### Dependencies Added

- `serde` + `serde_json` for JSON deserialization (add to Cargo.toml):
  ```toml
  serde = { version = "1", features = ["derive"] }
  serde_json = "1"
  ```

## Key Constants

| Constant       | Value     | Location       |
|----------------|-----------|----------------|
| PHYSICS_HZ     | 120       | physics.rs     |
| MAX_OBJECTS     | 2048      | renderer.rs    |
| LOAD_RADIUS_M  | 200000    | airport_gen.rs |
| UNIFORM_ALIGN   | 256       | renderer.rs    |
| PILOT_EYE_BODY | (2,0,-1)  | sim.rs         |
| FT_TO_M        | 0.3048    | airport_gen.rs |

## Build & Run

```bash
cargo run --release
```

Ensure `assets/airports/airports_all.json` exists. If missing, airports are
silently skipped (warning logged). OBJ landmarks live in `assets/obj_static/`.

## Controls

| Key            | Action                    |
|----------------|---------------------------|
| ↑/↓            | Elevator (pitch)          |
| ←/→            | Aileron (roll)            |
| Z/X            | Rudder (yaw)              |
| Shift/Ctrl     | Throttle up/down          |
| =/−            | Throttle up/down (alt)    |
| B              | Brakes                    |
| C              | Reset head look           |
| F11            | Toggle fullscreen         |
| Esc            | Release cursor / quit     |
| Mouse          | Head look (when grabbed)  |

## Notes

- `MAX_OBJECTS` in `renderer.rs` is 2048. Only airports within 200km
  (`LOAD_RADIUS_M`) of the aircraft start position are generated.
- Airport geometry uses flat normals per box face — the edge detector will
  fire on building edges as intended.