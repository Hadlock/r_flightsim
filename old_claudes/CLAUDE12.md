# CLAUDE.md — shaderflight: WGS84 Earth Rendering

## Project Context

shaderflight is a wgpu flight simulator with Sobel edge-detection wireframe rendering.
All world math is WGS-84 ECEF (f64). The current renderer draws white wireframe edges
on FSBLUE background `(0.10, 0.20, 0.30, 1.0)`. There is currently no horizon, no sky,
no ground plane — just objects floating in blue void.

This task adds a WGS-84 oblate spheroid mesh that serves as the earth's surface. At low
altitude it's a visible horizon line. At orbital distances it's the full globe. The mesh
participates in the Sobel edge detection pipeline so it renders as a wireframe sphere
consistent with the aesthetic.

Key constraint: **Do NOT modify `renderer.rs` or the WGSL shaders.** The earth mesh is
a normal `SceneObject` (or set of SceneObjects) that goes through the existing pipeline.
The Sobel edge detector will fire on the mesh edges, producing the horizon line and globe
outline automatically.

---

## Files

```
src/earth.rs         – Earth mesh generation, LOD management, SceneObject creation
```

Modify:
```
main.rs              – instantiate EarthRenderer, update LOD each frame, add to render list
camera.rs            – extend far plane at high altitudes (current far=40000 insufficient for orbit)
```

Do NOT modify: `renderer.rs`, shaders, `physics.rs`

---

## Core Idea

Generate an icosphere (or UV sphere) tessellated to approximate the WGS-84 oblate
spheroid. Each vertex is placed at the correct ECEF position for its lat/lon on the
ellipsoid surface. The mesh goes through the normal Sobel pipeline. At low altitude,
only the nearby horizon arc is visible (most triangles are behind the camera or culled).
At orbital altitude, the full sphere is visible as a wireframe globe.

Multiple LOD levels are pre-generated at startup. Each frame, pick the appropriate LOD
based on altitude and swap the active SceneObject's buffers.

---

## WGS-84 Ellipsoid Vertex Placement

Every vertex on the earth mesh is computed from geodetic lat/lon:

```rust
fn ellipsoid_vertex(lat_deg: f64, lon_deg: f64) -> DVec3 {
    coords::lla_to_ecef(&coords::LLA {
        lat: lat_deg.to_radians(),
        lon: lon_deg.to_radians(),
        alt: 0.0,
    })
}
```

This automatically produces the oblate spheroid shape (equatorial bulge, polar
flattening) because `lla_to_ecef` uses the real WGS-84 parameters. No need to
manually model the ellipsoid — it falls out of the coordinate math.

---

## Mesh Generation: UV Sphere (Lat/Lon Grid)

Use a latitude/longitude grid rather than an icosphere. Reasons:
- Trivial to generate at arbitrary resolution
- Latitude lines are natural horizon contours
- Easy to control density (more triangles near the viewer, fewer far away)
- Wireframe lat/lon grid looks like a globe, which suits the aesthetic

### Grid Parameters per LOD

```
LOD 0 (surface–5,000 ft):    2° spacing  →  180×90  = 16,200 quads = 32,400 tris
LOD 1 (5,000–30,000 ft):     4° spacing  →  90×45   =  4,050 quads =  8,100 tris
LOD 2 (30,000–100,000 ft):   6° spacing  →  60×30   =  1,800 quads =  3,600 tris
LOD 3 (100,000 ft–500 km):   10° spacing →  36×18   =    648 quads =  1,296 tris
LOD 4 (500 km–lunar):        15° spacing →  24×12   =    288 quads =    576 tris
```

Each quad = two triangles. These counts are very modest — even LOD 0 is only 32k tris,
which is nothing for a modern GPU.

### LOD Altitude Thresholds (meters)

```rust
const LOD_THRESHOLDS: [(f64, usize); 5] = [
    (1_524.0,    0),   //     0 – 5,000 ft:   LOD 0 (2°)
    (9_144.0,    1),   // 5,000 – 30,000 ft:  LOD 1 (4°)
    (30_480.0,   2),   // 30,000 – 100,000 ft: LOD 2 (6°)
    (500_000.0,  3),   // 100,000 ft – 500 km: LOD 3 (10°)
    (f64::MAX,   4),   // 500 km – lunar:      LOD 4 (15°)
];
```

These are loose guidelines. Tune based on what looks good — the wireframe density
should feel appropriate at each altitude. The Sobel detector will fire on triangle
edges, so denser mesh = more visible grid lines = more "ground texture" at low alt.

---

## Mesh Generation Code

```rust
pub struct EarthLod {
    pub vertices: Vec<Vertex>,   // obj_loader::Vertex
    pub indices: Vec<u32>,
    pub lat_step: f64,
    pub lon_step: f64,
}

pub fn generate_earth_lod(lat_step_deg: f64, lon_step_deg: f64) -> EarthLod {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let lat_steps = (180.0 / lat_step_deg).round() as i32;
    let lon_steps = (360.0 / lon_step_deg).round() as i32;

    // Generate vertices: (lat_steps + 1) rows × (lon_steps + 1) columns
    // Latitude: -90 to +90, Longitude: -180 to +180
    for i in 0..=lat_steps {
        let lat = -90.0 + (i as f64) * lat_step_deg;
        for j in 0..=lon_steps {
            let lon = -180.0 + (j as f64) * lon_step_deg;
            let ecef = ellipsoid_vertex(lat, lon);
            // Position stored as f32 — but this is relative to camera, computed at render time.
            // For the mesh template, store raw ECEF. See "Floating Origin" section below.
            vertices.push(ecef);
        }
    }

    // Generate indices: quads → two triangles each
    let cols = (lon_steps + 1) as u32;
    for i in 0..lat_steps as u32 {
        for j in 0..lon_steps as u32 {
            let tl = i * cols + j;
            let tr = i * cols + j + 1;
            let bl = (i + 1) * cols + j;
            let br = (i + 1) * cols + j + 1;
            // Two triangles per quad (CCW winding for front-face)
            indices.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
        }
    }

    EarthLod { vertices, indices, lat_step: lat_step_deg, lon_step: lon_step_deg }
}
```

---

## Floating Origin Problem

The earth mesh vertices are at ECEF coordinates (millions of meters from origin). The
renderer works with f32 positions relative to the camera. This is the same floating
origin approach used for all other SceneObjects — the `model_matrix_relative_to(camera_pos)`
method handles it.

**However**, the earth mesh is special because its vertices span the entire planet.
Vertices on the far side of the earth are ~12,000 km from the camera. At f32 precision,
that's fine for orbital viewing but loses sub-meter precision. For low-altitude viewing,
only nearby vertices matter, and those are close enough to the camera for f32 to work.

### Implementation

Store the earth mesh vertices as `DVec3` (f64) at their true ECEF positions. Each frame:

1. Determine camera position in ECEF
2. Pick the appropriate LOD
3. Rebuild the GPU vertex buffer with positions relative to camera:
   ```rust
   for vertex in &lod.vertices_ecef {
       let rel = *vertex - camera_pos;
       gpu_vertices.push(Vertex {
           position: [rel.x as f32, rel.y as f32, rel.z as f32],
           normal: /* computed from ellipsoid surface normal at this vertex */,
       });
   }
   ```
4. Upload to GPU and render as a SceneObject

**This per-frame vertex rebuild is the main cost.** For LOD 0 (32k tris, ~16k vertices),
rebuilding the buffer is a few hundred microseconds — fine at 60fps. For LOD 4 (~300 vertices),
it's negligible.

**Optimization**: Only rebuild when LOD changes or camera has moved significantly (>100m).
Cache the last buffer and reuse it across frames where the camera hasn't moved enough
to matter visually.

**Alternative approach**: Instead of rebuilding vertices, set the SceneObject's `world_pos`
to ECEF origin (0,0,0) and let the existing `model_matrix_relative_to` produce a
translation of `(0,0,0) - camera_pos = -camera_pos`. The vertices are stored in ECEF
in the GPU buffer. The translation matrix shifts everything. The problem: f32 overflow
for vertices far from the camera. This works at orbital altitudes (everything is far
anyway, precision doesn't matter) but fails at low altitude (nearby vertices need
precision). So: **use the per-frame relative rebuild for LOD 0-1, and the simple
translation approach for LOD 2-4.** This gives precision where needed and avoids
unnecessary work at altitude.

---

## Normals

Surface normals are needed for the Sobel edge detector to work. For the WGS-84 ellipsoid,
the outward surface normal at geodetic (lat, lon) is exactly the ENU "up" vector:

```rust
fn ellipsoid_normal(lat_rad: f64, lon_rad: f64) -> [f32; 3] {
    let (slat, clat) = lat_rad.sin_cos();
    let (slon, clon) = lon_rad.sin_cos();
    // This is the geodetic normal (perpendicular to ellipsoid surface)
    [
        (clat * clon) as f32,
        (clat * slon) as f32,
        slat as f32,
    ]
}
```

These normals are constant (they don't depend on camera position), so they can be
precomputed once and reused across frames.

---

## Far Plane Scaling

The current camera has `far: 40000.0` (40km). This is fine for GA flight but clips
the earth at orbital altitude. Scale the far plane dynamically based on altitude:

```rust
// In camera.rs or wherever the projection matrix is built:
fn dynamic_far_plane(altitude_m: f64) -> f32 {
    if altitude_m < 10_000.0 {
        40_000.0          // 40 km — normal flight
    } else if altitude_m < 100_000.0 {
        500_000.0         // 500 km — high altitude
    } else if altitude_m < 1_000_000.0 {
        5_000_000.0       // 5,000 km — suborbital/LEO
    } else if altitude_m < 50_000_000.0 {
        100_000_000.0     // 100,000 km — MEO/GEO
    } else {
        500_000_000.0     // 500,000 km — lunar distance (~384,400 km)
    }
}
```

Also scale the near plane up at extreme altitudes to preserve depth buffer precision:

```rust
fn dynamic_near_plane(altitude_m: f64) -> f32 {
    if altitude_m < 10_000.0 {
        1.0
    } else if altitude_m < 1_000_000.0 {
        100.0
    } else {
        10_000.0
    }
}
```

This keeps the depth buffer usable across the full range. At lunar distance, a 10km
near plane is fine — you can't see individual buildings from the moon.

---

## EarthRenderer Struct

```rust
pub struct EarthRenderer {
    // Pre-generated LOD meshes (vertices in ECEF f64)
    lods: Vec<EarthLodData>,
    // Current LOD level
    current_lod: usize,
    // GPU buffers for current LOD
    scene_object: SceneObject,
    // Cached camera position for rebuild-on-move optimization
    last_camera_ecef: DVec3,
    last_rebuild_lod: usize,
}

struct EarthLodData {
    vertices_ecef: Vec<DVec3>,   // f64 ECEF positions
    normals: Vec<[f32; 3]>,      // precomputed surface normals
    indices: Vec<u32>,
    vertex_count: usize,
}
```

### Per-Frame Update

```rust
impl EarthRenderer {
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera_pos_ecef: DVec3,
        altitude_m: f64,
    ) {
        // 1. Pick LOD
        let new_lod = select_lod(altitude_m);

        // 2. Check if rebuild needed
        let camera_moved = (camera_pos_ecef - self.last_camera_ecef).length();
        let needs_rebuild = new_lod != self.current_lod
            || (new_lod <= 1 && camera_moved > 100.0)  // high-detail LODs: rebuild on move
            || (new_lod >= 2 && camera_moved > 10_000.0); // low-detail: less frequent

        if !needs_rebuild {
            return;
        }

        // 3. Rebuild vertex buffer with camera-relative positions
        let lod = &self.lods[new_lod];
        let vertices: Vec<Vertex> = lod.vertices_ecef.iter()
            .zip(lod.normals.iter())
            .map(|(pos, normal)| {
                let rel = *pos - camera_pos_ecef;
                Vertex {
                    position: [rel.x as f32, rel.y as f32, rel.z as f32],
                    normal: *normal,
                }
            })
            .collect();

        // 4. Upload to GPU (recreate buffer or write to mapped buffer)
        // Recreating is simpler and fine for the rebuild frequency:
        self.scene_object.vertex_buf = device.create_buffer_init(...);
        // Index buffer doesn't change within a LOD, only on LOD switch
        if new_lod != self.current_lod {
            self.scene_object.index_buf = device.create_buffer_init(...);
            self.scene_object.index_count = lod.indices.len() as u32;
        }

        self.current_lod = new_lod;
        self.last_camera_ecef = camera_pos_ecef;
        self.last_rebuild_lod = new_lod;
    }
}
```

### Rendering

The earth `SceneObject` is rendered like any other object. Since vertices are already
camera-relative, set `world_pos = camera_pos` (so `model_matrix_relative_to` produces
a zero translation) and `rotation = Quat::IDENTITY`, `scale = 1.0`.

Actually simpler: set `world_pos = DVec3::ZERO` and override the model matrix to be
identity (since vertices are already in camera-relative space). Or just add the earth
vertices pre-transformed and set `world_pos = camera_pos_ecef`. Either works — pick
whichever integrates cleanly with the existing render loop.

---

## Object ID and Edge Detection

Give the earth mesh a unique `object_id` (e.g., `2`). The Sobel edge detector fires
on depth/normal discontinuities AND on object ID boundaries. This means:

- The horizon line appears where earth meets sky (depth discontinuity at the edge)
- Grid lines appear where adjacent triangles have different normals (the lat/lon grid
  curvature creates slight normal changes at each edge)
- Where aircraft or buildings overlap the earth, the object ID boundary creates an edge

For the grid lines to show up nicely in the Sobel pass, the normals need to be **flat
per-face** (not smoothed). If normals are smoothed across the sphere, the Sobel detector
won't see the grid edges. Use flat normals (each triangle has a single face normal
for all three vertices). This makes the wireframe lat/lon grid visible.

**Important**: The existing `obj_loader.rs` computes smooth normals by position. The
earth mesh bypasses `obj_loader` entirely (generated procedurally), so you control the
normals directly. Use flat face normals to get visible grid lines, or smooth normals
for a cleaner horizon-only look. Recommend: **flat normals** — the wireframe grid on
the earth is part of the aesthetic and gives the retro-globe look.

---

## Backface Culling Note

The existing renderer likely has backface culling enabled. The earth mesh triangles
facing away from the camera (far side of the globe) will be culled automatically.
This is exactly what we want:
- At low altitude: only the nearby ground surface renders (horizon arc)
- At orbital altitude: the visible hemisphere renders (half the globe)
- The far side never renders, saving ~50% of the triangle count

Ensure winding order is consistent (CCW for front-facing when viewed from outside
the ellipsoid). The generation code above uses CCW winding.

---

## Visual Result by Altitude

**5,000 ft**: Dense lat/lon grid visible below, curving away to a clean horizon line.
The wireframe grid gives the ground visual texture. Sky above the horizon is FSBLUE void.

**30,000 ft**: Coarser grid, wider view. Earth curvature is subtly visible. Horizon
is a gentle arc.

**100,000 ft**: Earth curvature is obvious. Grid is sparse. Starting to see the globe shape.

**LEO (400 km)**: Full hemisphere visible. Wireframe globe. Equatorial bulge is technically
present but not visually obvious at this distance.

**GEO (35,786 km)**: Small wireframe sphere floating in FSBLUE void. Classic "earth from
space" wireframe look.

**Lunar (384,400 km)**: Tiny wireframe sphere. The oblate flattening (1/298) is about
21 km difference between equatorial and polar radius — not visible at this distance,
but geometrically correct.

---

## Performance Notes

- LOD 0 (highest detail): ~16k vertices rebuilt per frame when moving. At ~64 bytes per
  vertex (position + normal), that's ~1MB of data. Buffer creation is fast on M3.
- LOD 4 (lowest detail): ~300 vertices. Negligible.
- The rebuild-on-move threshold (100m for LOD 0-1, 10km for LOD 2+) means rebuilds are
  infrequent during cruise. During dynamic maneuvering at low altitude, rebuilds happen
  every few frames — still fine.
- Total GPU memory for all LOD index buffers: trivial (few hundred KB across all LODs).
  Vertex buffers are rebuilt in-place, so only one vertex buffer exists at a time.

---

## MAX_OBJECTS Check

The renderer has `MAX_OBJECTS = 128`. The earth is one additional SceneObject. Ensure
there's headroom. Current count: scene objects (~10-15) + airport geometry (~20-40) +
aircraft (1) + AI traffic (7) = ~40-60. Plenty of room.

---

## Do NOT

- Modify `renderer.rs` or shaders
- Use terrain data or heightmaps (future work — this is just the smooth ellipsoid)
- Generate more than ~50k triangles for any LOD level
- Store f32 ECEF positions directly (precision loss — always compute relative to camera)
- Use a sphere approximation — use the real WGS-84 ellipsoid via `lla_to_ecef`
- Forget to handle the date line / pole singularities in the UV sphere (longitude wraps
  at ±180°, latitude terminates at ±90° — standard UV sphere handling)

---

## Future Hooks

This earth mesh is the foundation for:
- **Day/night terminator**: Compute sun position, shade triangles on the dark side differently
- **Terrain**: Perturb vertex altitudes using elevation data (SRTM/ETOPO)
- **Coastlines**: Additional line geometry overlaid on the earth mesh
- **Atmosphere glow**: Rim lighting at the horizon edge (would need shader changes — future)
- **Space missions**: The earth is correctly positioned and scaled for orbital mechanics

---

## Verification

After implementation:
- At SFO (sea level): visible grid on the ground, clean horizon line separating earth from sky
- At 30,000 ft: earth curvature visible, coarser grid
- At 100 km: hemisphere visible, wireframe globe forming
- At GEO: full wireframe earth sphere visible, correctly oblate
- LOD transitions are not jarring (grid density changes smoothly enough)
- No z-fighting at any altitude (near/far plane scaling works)
- No precision artifacts (jittering vertices) at low altitude
- Performance stays >60fps across all altitudes
- `cargo run --release -- -i` then fly straight up: watch the transition from horizon to globe