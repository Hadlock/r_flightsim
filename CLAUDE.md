# CLAUDE.md — Landmarks, OBJ Origin System, Pyramid Generation

## Overview
Add landmark objects to the scene: three existing cubes and three new pyramids. Implement an origin-tag system that reads `# origin:` comments from OBJ files to automatically place them in the world via ECEF coordinates.

## Task 1: Generate Three Pyramid OBJ Files

Create these files in `assets/`. Each pyramid is a square-base pyramid (5 vertices, 6 triangular faces). The OBJ geometry is centered at (0,0,0) in local coordinates, using ENU convention for the vertex layout: X=east, Y=north, Z=up. The base sits on Z=0 (ground), apex at Z=height.

### Pyramid 1: Great Pyramid (Giza-scale)
**File:** `assets/pyramid_giza.obj`
```
# Great Pyramid landmark
# origin: 37°36'57.5"N 122°23'16.6"W
# base: 230m, height: 150m
```
- Base corners at Z=0: (±115, ±115, 0) — four combinations
- Apex at (0, 0, 150)
- Half-base = 115.0

### Pyramid 2: Transamerica-scale
**File:** `assets/pyramid_transamerica.obj`
```
# Transamerica Pyramid landmark
# origin: 37.795200, -122.402800
# base: 53m, height: 260m
```
- Base corners at Z=0: (±26.5, ±26.5, 0)
- Apex at (0, 0, 260)
- Half-base = 26.5

### Pyramid 3: Mountain-scale
**File:** `assets/pyramid_mountain.obj`
```
# Mountain pyramid landmark
# origin: 37°55'44.5"N 122°34'40.0"W
# base: 10000m, height: 784m
```
- Base corners at Z=0: (±5000, ±5000, 0)
- Apex at (0, 0, 784)
- Half-base = 5000.0

### OBJ Vertex/Face Template
All three pyramids use the same topology, just different dimensions. For half-base `h` and height `H`:

```obj
# [title comment]
# origin: [coordinates]
v -h -h 0.0
v  h -h 0.0
v  h  h 0.0
v -h  h 0.0
v  0.0  0.0 H

vn 0 0 -1
# face normals computed below

# Base (two triangles, facing down, normal -Z)
f 1//1 3//1 2//1
f 1//1 4//1 3//1

# South face (v1, v2, v5)
f 1 2 5
# East face (v2, v3, v5)
f 2 3 5
# North face (v3, v4, v5)
f 3 4 5
# West face (v4, v1, v5)
f 4 1 5
```

Note: Don't include normals on the side faces — let `obj_loader.rs` compute smooth normals (it already does this). Only include the base normal explicitly if desired, or just let the smooth normal pass handle everything. Simplest: omit all `vn` lines and `//n` references, let the loader compute normals.

Simplest OBJ format (no normals, loader computes them):
```obj
# [title]
# origin: 37°36'57.5"N 122°23'16.6"W
v -115.0 -115.0 0.0
v  115.0 -115.0 0.0
v  115.0  115.0 0.0
v -115.0  115.0 0.0
v  0.0  0.0  150.0
f 1 3 2
f 1 4 3
f 1 2 5
f 2 3 5
f 3 4 5
f 4 1 5
```

## Task 2: Rename Cube Files

Rename in `assets/`:
- `1m_cube.txt` → `1m_cube.obj`
- `10m_cube.txt` → `10m_cube.obj`
- `30m_cube.txt` → `30m_cube.obj`

## Task 3: Origin Tag Parser

Create a new function in `scene.rs` (or a small helper module) that:

1. Reads the first ~10 lines of an OBJ file looking for `# origin:` comments
2. Parses the coordinates in either format:
   - DMS: `37°36'57.5"N 122°23'16.6"W`
   - Decimal: `37.795200, -122.402800`
3. Returns an `Option<LLA>` (alt = 0.0 for ground objects)

### Parser Details

```rust
/// Parse an origin comment from an OBJ file.
/// Supports two formats:
///   # origin: 37°36'57.5"N 122°23'16.6"W
///   # origin: 37.795200, -122.402800
pub fn parse_origin(path: &Path) -> Option<LLA>
```

**DMS parsing:**
- Pattern: `DD°MM'SS.S"N/S DDD°MM'SS.S"E/W`
- Degrees = DD + MM/60 + SS.S/3600
- S and W are negative
- The degree symbol may be `°` (UTF-8) or similar

**Decimal parsing:**
- Pattern: two comma-separated floats
- First is latitude, second is longitude
- Negative values for S/W (no N/S/E/W suffix)

Both formats: altitude defaults to 0.0 (ground level on ellipsoid).

**Important:** Return latitude and longitude in **radians** in the LLA struct (matching the existing convention in coords.rs).

## Task 4: Update `scene.rs` — Auto-Load All OBJ Landmarks

Replace the current hardcoded teapot loading in `load_scene()` with a system that:

1. Scans `assets/` directory for all `.obj` files
2. Skips the aircraft OBJ (hardcoded name or a skip-list)
3. For each OBJ with a valid `# origin:` tag:
   - Parse the origin → LLA → ECEF position
   - Load the mesh
   - Place it in the world at that ECEF position
   - The OBJ local frame is ENU (X=east, Y=north, Z=up), so the rotation must transform from local ENU to ECEF at that location

### ENU-to-ECEF Rotation for Static Objects

This is critical. The OBJ vertices are in ENU coordinates (X=east, Y=north, Z=up) at the object's origin. To render them correctly in ECEF world space, the SceneObject rotation must encode the ENU→ECEF rotation at that lat/lon.

```rust
fn enu_to_ecef_quat(lat_rad: f64, lon_rad: f64) -> Quat {
    let enu = coords::enu_frame_at(lat_rad, lon_rad, DVec3::ZERO);
    // Build rotation matrix: columns are where ENU X,Y,Z axes go in ECEF
    // ENU X (east) → enu.east in ECEF
    // ENU Y (north) → enu.north in ECEF
    // ENU Z (up) → enu.up in ECEF
    let mat = glam::DMat3::from_cols(enu.east, enu.north, enu.up);
    let dq = glam::DQuat::from_mat3(&mat);
    Quat::from_xyzw(dq.x as f32, dq.y as f32, dq.z as f32, dq.w as f32)
}
```

Each static SceneObject gets:
- `world_pos`: ECEF position from `lla_to_ecef(origin_lla)`
- `rotation`: `enu_to_ecef_quat(origin_lla.lat, origin_lla.lon)`  
- `scale`: `1.0` (OBJ vertices are already in meters)

### Placement Relative to Runway

The aircraft starts at SFO heading 280°. Looking out the windshield:
- **Left side (south of runway):** Place the three cubes
- **Right side (north of runway):** The teapots are already there

The cubes have their own `# origin:` comments already. Verify the cube origins put them to the left (south) of the runway centerline. If they need adjusting, update the origin comments in the cube OBJ files. The cubes should be visible from the cockpit during the takeoff roll.

Current cube origins from the files:
- 1m cube: `37°38'37.3"N 122°34'18.1"W` — this is ~5km northwest, might be too far. Move closer if needed.
- 10m cube: `37.643700, -122.571700` — this is way west over the ocean. Fix this.
- 30m cube: `37°36'53.1"N 122°21'56.1"W` — east of SFO

**Update cube origins** to be south of the runway, visible during takeoff roll at SFO 28L. The runway is at approximately 37.6139°N, 122.358°W heading 280°. Place cubes ~50-200m south of centerline, spaced along the runway:

```
# 1m cube — near threshold, 50m south of centerline
# origin: 37.6135, -122.3575

# 10m cube — 300m down runway, 100m south  
# origin: 37.6130, -122.3610

# 30m cube — 600m down runway, 150m south
# origin: 37.6125, -122.3650
```

Update the `# origin:` comments in the three cube OBJ files to these positions.

### Keep Teapots
Keep teapots in the scene too. They already have hardcoded positions (10m east of reference, every 100m north). Convert teapots to also use the origin-tag system, OR keep them hardcoded — either is fine. The important thing is they remain visible to the right during takeoff.

## Task 5: Update `obj_loader.rs` — Handle Missing Normals

The pyramid OBJs won't have normals. The loader already computes smooth normals as a fallback, so this should just work. But verify: if an OBJ has no `vn` lines, the loader should still produce valid vertices with computed normals. The current code initializes normals to `[0,0,0]` and then overwrites with computed smooth normals — this is correct.

## Summary of File Changes

### New files:
- `assets/pyramid_giza.obj`
- `assets/pyramid_transamerica.obj`
- `assets/pyramid_mountain.obj`

### Renamed files:
- `assets/1m_cube.txt` → `assets/1m_cube.obj` (also update origin comment)
- `assets/10m_cube.txt` → `assets/10m_cube.obj` (also update origin comment)
- `assets/30m_cube.txt` → `assets/30m_cube.obj` (also update origin comment)

### Modified files:
- `scene.rs` — origin parser, auto-loading, ENU→ECEF rotation for static objects
- `main.rs` — update `load_scene()` call if signature changes (it currently takes ref_pos and enu, may no longer need those if we auto-parse origins)

### NOT modified:
- `physics.rs`, `coords.rs`, `camera.rs`, `renderer.rs`, `obj_loader.rs`, `sim.rs`, `shaders/*`

## Test
1. `cargo run --release`
2. Sitting on SFO 28L, cubes visible to the left, teapots to the right
3. Throttle up, roll down runway — cubes and teapots scroll past
4. After liftoff and climbing, pyramids should be visible:
   - Giza pyramid: ~5km south-southeast, 150m tall
   - Transamerica: ~20km north in SF, 260m tall, thin
   - Mountain: ~35km northwest, massive 10km base
5. All objects should be right-side-up (Z=up in ENU correctly maps to local vertical)