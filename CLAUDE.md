# CLAUDE.md — shaderflight: Celestial Mechanics

## Project Context

shaderflight is a wgpu flight simulator with Sobel edge-detection wireframe rendering.
All world math is WGS-84 ECEF (f64). There is now a WGS-84 earth mesh with LOD levels
from surface to lunar distance. The far plane scales dynamically with altitude.

This task adds: sun, moon, 7 planets, and ~500 bright stars, all positioned correctly
in ECEF space based on the current sim time. The moon must be accurately positioned
enough to fly to and orbit (within ~100 km positional accuracy). The sun and planets
are visual-accuracy only for v1. Stars are rendered from the Hipparcos bright star
catalog.

---

## Files

```
src/celestial/mod.rs         – CelestialEngine: time, update loop, public API
src/celestial/time.rs        – SimClock, Julian Date, GMST, sidereal time
src/celestial/sun.rs         – Solar position (simplified SPA)
src/celestial/moon.rs        – Lunar position (truncated ELP2000 or equivalent)
src/celestial/planets.rs     – Planetary positions (simplified VSOP87 or orbital elements)
src/celestial/stars.rs       – Star catalog loading and J2000→ECEF transform
src/celestial/bodies.rs      – CelestialBody struct, mesh generation for sun/moon/planets
```

Modify:
```
main.rs          – instantiate CelestialEngine, update each frame, add bodies to scene
camera.rs        – extend far plane for sun distance (already partially done for earth)
cli.rs           – add --epoch flag for custom start time
```

Do NOT modify: `renderer.rs`, shaders, `physics.rs`

---

## 1. Simulation Clock

### SimClock

```rust
pub struct SimClock {
    /// Current sim time as Julian Date (TT, terrestrial time)
    jd: f64,
    /// UTC offset from TT (≈69.184 seconds in 2025, close enough to ignore for visual work)
    /// We treat UTC ≈ TT for simplicity. Sub-second accuracy is irrelevant.
    epoch_unix: f64,       // Unix timestamp of the sim epoch
    elapsed_sim: f64,      // seconds of sim time elapsed since epoch
    time_scale: f64,       // 1.0 = real-time, 10.0 = 10x warp, etc.
}
```

### Time Source

- Default: system clock (`std::time::SystemTime::now()` → Unix epoch → Julian Date)
- Custom epoch via CLI: `--epoch "2025-06-15T14:30:00Z"`
- Once running, sim time advances at `time_scale × wall_clock_dt` per frame
- `time_scale` is 1.0 for now (real-time). Time warp is future work but the
  architecture supports it by just changing this value.

### Julian Date Conversion

```rust
/// Unix timestamp (seconds since 1970-01-01T00:00:00Z) to Julian Date
fn unix_to_jd(unix_secs: f64) -> f64 {
    // J2000.0 = 2451545.0 JD = 2000-01-01T12:00:00 TT
    // Unix epoch = 2440587.5 JD
    2_440_587.5 + unix_secs / 86_400.0
}

/// Julian centuries since J2000.0
fn jd_to_t(jd: f64) -> f64 {
    (jd - 2_451_545.0) / 36_525.0
}
```

### Greenwich Mean Sidereal Time

```rust
/// GMST in degrees from Julian Date
fn gmst_deg(jd: f64) -> f64 {
    let t = jd_to_t(jd);
    let gmst = 280.46061837
        + 360.98564736629 * (jd - 2_451_545.0)
        + 0.000387933 * t * t
        - t * t * t / 38_710_000.0;
    gmst.rem_euclid(360.0)
}
```

This is the Earth rotation angle — the key transform from J2000 inertial to ECEF.

### CLI Addition

```rust
// In cli.rs Args:
/// Simulation start time (ISO 8601 UTC). Defaults to system clock.
#[arg(long = "epoch")]
pub epoch: Option<String>,
```

Parse with `chrono` or manual ISO 8601 parsing. If not provided, use `SystemTime::now()`.

---

## 2. Coordinate Pipeline

All celestial computations follow this pipeline:

```
Analytical model → Ecliptic (J2000) → Equatorial (J2000) → ECEF → SceneObject
```

### Ecliptic → Equatorial (J2000)

The obliquity of the ecliptic (angle between ecliptic and equatorial planes):

```rust
/// Mean obliquity of the ecliptic at epoch T (Julian centuries from J2000)
fn obliquity_deg(t: f64) -> f64 {
    23.439_291 - 0.013_004_2 * t  // good enough for centuries around J2000
}
```

Rotation from ecliptic to equatorial:
```rust
fn ecliptic_to_equatorial(lon_rad: f64, lat_rad: f64, dist: f64, obliquity_rad: f64) -> DVec3 {
    // Ecliptic Cartesian
    let x = dist * lat_rad.cos() * lon_rad.cos();
    let y = dist * lat_rad.cos() * lon_rad.sin();
    let z = dist * lat_rad.sin();
    // Rotate around X axis by obliquity
    let cos_e = obliquity_rad.cos();
    let sin_e = obliquity_rad.sin();
    DVec3::new(
        x,
        y * cos_e - z * sin_e,
        y * sin_e + z * cos_e,
    )
}
```

### Equatorial (J2000 ECI) → ECEF

Earth rotates under the stars. The GMST gives the rotation angle:

```rust
fn eci_to_ecef(eci: DVec3, gmst_rad: f64) -> DVec3 {
    let cos_g = gmst_rad.cos();
    let sin_g = gmst_rad.sin();
    DVec3::new(
        eci.x * cos_g + eci.y * sin_g,
        -eci.x * sin_g + eci.y * cos_g,
        eci.z,
    )
}
```

This single rotation is what makes the sun rise and set, the moon track across the sky,
and the stars rotate overhead. It's the core of the whole system.

---

## 3. Sun Position

Use the simplified solar position algorithm. Accurate to ~0.01° (more than sufficient).

```rust
pub fn sun_position(jd: f64) -> SunResult {
    let n = jd - 2_451_545.0;  // days since J2000.0
    let t = n / 36_525.0;

    // Mean longitude (degrees)
    let l0 = (280.46646 + 36_000.76983 * t + 0.0003032 * t * t).rem_euclid(360.0);
    // Mean anomaly (degrees)
    let m = (357.52911 + 35_999.05029 * t - 0.0001537 * t * t).rem_euclid(360.0);
    let m_rad = m.to_radians();

    // Equation of center
    let c = 1.914_602 * m_rad.sin()
          + 0.019_993 * (2.0 * m_rad).sin()
          + 0.000_289 * (3.0 * m_rad).sin();

    // Sun's ecliptic longitude
    let sun_lon = (l0 + c).to_radians();
    let sun_lat = 0.0_f64; // Sun is always on the ecliptic (lat ≈ 0)

    // Distance (AU)
    let e = 0.016_708_634 - 0.000_042_037 * t;
    let v = m_rad + c.to_radians(); // true anomaly
    let r_au = 1.000_001_018 * (1.0 - e * e) / (1.0 + e * v.cos());
    let r_m = r_au * 149_597_870_700.0; // convert to meters

    // Convert to equatorial J2000
    let obliquity = obliquity_deg(t).to_radians();
    let eci = ecliptic_to_equatorial(sun_lon, sun_lat, r_m, obliquity);

    SunResult { eci, distance_m: r_m }
}
```

### Sun Rendering

The sun subtends ~0.533° as seen from Earth. At its true distance (~150M km),
this means a diameter of ~1.39M km. Place a sphere of this diameter at the sun's
ECEF position.

**The "filled sun" problem:** The Sobel pipeline only draws wireframe edges. To make
the sun appear filled/solid white (distinguishable from the hollow wireframe moon),
generate the sun sphere with very high tessellation — an icosphere with 5-6 subdivisions
(~10,000+ triangles). At the angular size of 0.5°, the wireframe triangle edges will be
sub-pixel, making the sphere appear as a solid white disc. The Sobel detector fires on
every edge, and when edges are denser than pixel spacing, the result is a solid fill.

```rust
const SUN_SUBDIVISIONS: u32 = 6;  // ~10,000 tris, renders as solid disc
```

**Far plane for the sun:** The sun is ~150M km away. The dynamic far plane from the earth
rendering work caps at 500,000 km (lunar distance). Extend it:

```rust
fn dynamic_far_plane(altitude_m: f64) -> f32 {
    if altitude_m < 50_000_000.0 {
        // ... existing earth/lunar tiers ...
        500_000_000.0       // 500,000 km
    } else {
        200_000_000_000.0   // 200M km — encompasses inner solar system
    }
}
```

**Alternatively:** render the sun as "always on top" by disabling depth test for just
that object — but that requires renderer changes. The far plane approach is simpler
and stays within the existing pipeline. At the altitudes where you can see the sun
(always, really), the far plane will be scaled appropriately.

**Practical concern:** At 150M km distance with f32 vertex positions relative to camera,
precision is ~10 km per float step. That's fine for a sphere that's 1.39M km across —
the vertices won't jitter. But the translation (camera to sun) is 150M km which doesn't
fit in f32 at all. So: the sun SceneObject must use the same camera-relative vertex
rebuild approach as the earth mesh. Compute `vertex_ecef - camera_ecef` per vertex
each frame. For a 10k tri sphere this is negligible cost.

Actually, simpler: since the sun is so far away, render it at a **fixed large distance**
(say 100M meters = 100,000 km) in the correct direction, scaled to subtend 0.533°.
This is the standard skybox trick — distant objects don't need to be at their true
distance, just in the right direction at the right angular size. Render before
depth-testing scene objects, or just put it far enough that it's behind everything
terrestrial.

```rust
const SUN_RENDER_DISTANCE: f64 = 100_000_000.0;  // 100,000 km (well inside far plane)
let sun_direction = (sun_ecef - camera_ecef).normalize();
let sun_render_pos = camera_ecef + sun_direction * SUN_RENDER_DISTANCE;
let sun_angular_diameter_rad = 0.00930;  // 0.533° in radians
let sun_render_radius = SUN_RENDER_DISTANCE * (sun_angular_diameter_rad / 2.0).tan();
```

This sidesteps all f32 precision issues. Do the same for the moon and planets.

---

## 4. Moon Position

The moon needs higher accuracy than the sun because it's a navigation target. Use a
truncated series expansion. The full ELP2000 has thousands of terms; the first ~30-50
terms give accuracy to ~0.1° (position accuracy ~700 km at lunar distance). That's
sufficient for visual navigation and orbit insertion planning.

### Simplified Lunar Model

```rust
pub fn moon_position(jd: f64) -> MoonResult {
    let t = jd_to_t(jd);

    // Fundamental arguments (degrees)
    // L' = mean longitude of moon
    let lp = (218.3164477 + 481267.88123421 * t
              - 0.0015786 * t * t).rem_euclid(360.0);
    // D = mean elongation of moon
    let d = (297.8501921 + 445267.1114034 * t
             - 0.0018819 * t * t).rem_euclid(360.0);
    // M = mean anomaly of sun
    let m = (357.5291092 + 35999.0502909 * t
             - 0.0001536 * t * t).rem_euclid(360.0);
    // M' = mean anomaly of moon
    let mp = (134.9633964 + 477198.8675055 * t
              + 0.0087414 * t * t).rem_euclid(360.0);
    // F = moon's argument of latitude
    let f = (93.2720950 + 483202.0175233 * t
             - 0.0036539 * t * t).rem_euclid(360.0);

    let d_r = d.to_radians();
    let m_r = m.to_radians();
    let mp_r = mp.to_radians();
    let f_r = f.to_radians();

    // Longitude terms (truncated series — top ~20 terms)
    // These are the largest-amplitude terms from ELP2000
    let sum_l = 6_288_774.0 * mp_r.sin()
        + 1_274_027.0 * (2.0 * d_r - mp_r).sin()
        + 658_314.0 * (2.0 * d_r).sin()
        + 213_618.0 * (2.0 * mp_r).sin()
        - 185_116.0 * m_r.sin()
        - 114_332.0 * (2.0 * f_r).sin()
        + 58_793.0 * (2.0 * d_r - 2.0 * mp_r).sin()
        + 57_066.0 * (2.0 * d_r - m_r - mp_r).sin()
        + 53_322.0 * (2.0 * d_r + mp_r).sin()
        + 45_758.0 * (2.0 * d_r - m_r).sin()
        - 40_923.0 * (m_r - mp_r).sin()
        - 34_720.0 * d_r.sin()
        - 30_383.0 * (m_r + mp_r).sin()
        + 15_327.0 * (2.0 * d_r - 2.0 * f_r).sin()
        - 12_528.0 * (mp_r + 2.0 * f_r).sin()
        + 10_980.0 * (mp_r - 2.0 * f_r).sin()
        + 10_675.0 * (4.0 * d_r - mp_r).sin()
        + 10_034.0 * (3.0 * mp_r).sin()
        + 8_548.0 * (4.0 * d_r - 2.0 * mp_r).sin()
        - 7_888.0 * (2.0 * d_r + m_r - mp_r).sin()
        - 6_766.0 * (2.0 * d_r + m_r).sin()
        - 5_163.0 * (d_r - mp_r).sin()
        + 4_987.0 * (d_r + m_r).sin()
        + 4_036.0 * (2.0 * d_r - m_r + mp_r).sin();

    // Latitude terms
    let sum_b = 5_128_122.0 * f_r.sin()
        + 280_602.0 * (mp_r + f_r).sin()
        + 277_693.0 * (mp_r - f_r).sin()
        + 173_237.0 * (2.0 * d_r - f_r).sin()
        + 55_413.0 * (2.0 * d_r - mp_r + f_r).sin()
        + 46_271.0 * (2.0 * d_r - mp_r - f_r).sin()
        + 32_573.0 * (2.0 * d_r + f_r).sin()
        + 17_198.0 * (2.0 * mp_r + f_r).sin()
        + 9_266.0 * (2.0 * d_r + mp_r - f_r).sin()
        + 8_822.0 * (2.0 * mp_r - f_r).sin();

    // Distance terms (km, as corrections to mean distance)
    let sum_r = -20_905_355.0 * mp_r.cos()
        - 3_699_111.0 * (2.0 * d_r - mp_r).cos()
        - 2_955_968.0 * (2.0 * d_r).cos()
        - 569_925.0 * (2.0 * mp_r).cos()
        + 48_888.0 * m_r.cos()
        - 3_149.0 * (2.0 * f_r).cos()
        + 246_158.0 * (2.0 * d_r - 2.0 * mp_r).cos()
        - 152_138.0 * (2.0 * d_r - m_r - mp_r).cos()
        - 170_733.0 * (2.0 * d_r + mp_r).cos()
        - 204_586.0 * (2.0 * d_r - m_r).cos()
        - 129_620.0 * (m_r - mp_r).cos()
        + 108_743.0 * d_r.cos();

    // Ecliptic longitude and latitude
    let lon_deg = lp + sum_l / 1_000_000.0;
    let lat_deg = sum_b / 1_000_000.0;
    let dist_km = 385_000.56 + sum_r / 1_000.0;
    let dist_m = dist_km * 1_000.0;

    // Convert to equatorial J2000
    let obliquity = obliquity_deg(t).to_radians();
    let eci = ecliptic_to_equatorial(
        lon_deg.to_radians(),
        lat_deg.to_radians(),
        dist_m,
        obliquity,
    );

    MoonResult {
        eci,
        distance_m: dist_m,
        diameter_m: 3_474_800.0,   // mean lunar diameter
    }
}
```

This truncated series gives ~0.1° positional accuracy (~700 km at lunar distance).
For navigation to the moon, this is the weakest link. If more accuracy is needed later,
add more terms from the ELP2000 tables — each additional term improves precision.
For v1 this is sufficient: you can see the moon, fly toward it, and enter a reasonable
orbit. Fine trajectory planning for Apollo-style free-return trajectories would want
~0.01° accuracy (~70 km), achievable by expanding to ~100 terms.

### Moon Rendering

The moon subtends ~0.517° from Earth's surface. Diameter: 3,474.8 km.

Render as a **moderately tessellated** sphere (icosphere 3-4 subdivisions, ~1000-2000 tris).
The wireframe edges will be visible, giving it the hollow/skeletal wireframe look
distinct from the filled sun. This is the visual distinction between sun and moon
without needing shader changes.

Use the same render-distance trick as the sun:
```rust
const MOON_RENDER_DISTANCE: f64 = 50_000_000.0;  // 50,000 km
let moon_direction = (moon_ecef - camera_ecef).normalize();
let moon_render_pos = camera_ecef + moon_direction * MOON_RENDER_DISTANCE;
let moon_angular_diameter = 2.0 * (MOON_DIAMETER / (2.0 * moon_true_distance)).atan();
let moon_render_radius = MOON_RENDER_DISTANCE * (moon_angular_diameter / 2.0).tan();
```

**Exception: when flying to the moon.** When the camera is within ~1,000,000 km of
the moon, switch from the angular-size trick to true-distance rendering. Place the
moon sphere at its actual ECEF position with its actual diameter (3,474.8 km). At
close range, the camera-relative vertex rebuild handles f32 precision (same approach
as the earth mesh). The LOD for the moon sphere should increase as you approach —
coarse wireframe from Earth, denser mesh up close.

```rust
const MOON_TRUE_RENDER_THRESHOLD: f64 = 1_000_000_000.0; // 1M km — switch to true position
```

### Moon LOD (for lunar orbit missions)

```
Distance > 1M km:     icosphere subdivisions=3 (~500 tris), angular-size rendering
100,000–1M km:         subdivisions=4 (~2000 tris), true position
10,000–100,000 km:    subdivisions=5 (~8000 tris), true position
< 10,000 km:          subdivisions=6 (~32000 tris), true position, surface detail visible
```

At low lunar orbit (~100 km altitude), the moon surface fills the view like the earth
does at normal flight altitude. The wireframe grid on the moon sphere looks like lunar
terrain viewed through the Sobel aesthetic. No actual terrain data needed — the geometry
of the sphere plus the wireframe is the visual.

---

## 5. Planets

Visual accuracy only. Use mean orbital elements evaluated at the current epoch. Accurate
to ~1° over years, which is more than enough for "that bright dot is Jupiter."

### Keplerian Orbital Elements (J2000 epoch + rates)

Store mean elements and rates for each planet:

```rust
pub struct PlanetElements {
    pub name: &'static str,
    // Elements at J2000.0
    pub a: f64,         // semi-major axis (AU)
    pub e: f64,         // eccentricity
    pub i: f64,         // inclination (degrees)
    pub omega: f64,     // longitude of ascending node (degrees)
    pub w_bar: f64,     // longitude of perihelion (degrees)
    pub l: f64,         // mean longitude (degrees)
    // Rates per Julian century
    pub a_dot: f64,
    pub e_dot: f64,
    pub i_dot: f64,
    pub omega_dot: f64,
    pub w_bar_dot: f64,
    pub l_dot: f64,
}
```

Use the JPL approximate planetary positions tables (Standish 1992, available in
NASA technical memorandums). These give elements + rates for all 8 planets valid
for several centuries around J2000.

Solve Kepler's equation iteratively (Newton's method, ~5 iterations) to get the
true anomaly, then compute heliocentric ecliptic position, then transform to
equatorial J2000, then to ECEF.

### Planet Data Table

```rust
// Mercury through Neptune — mean elements at J2000.0 and rates per century
// Source: JPL Solar System Dynamics, Standish (1992)
const PLANETS: [PlanetElements; 8] = [
    PlanetElements { name: "Mercury", a: 0.38709927, e: 0.20563593, i: 7.00497902,
        omega: 48.33076593, w_bar: 77.45779628, l: 252.25032350,
        a_dot: 0.00000037, e_dot: 0.00001906, i_dot: -0.00594749,
        omega_dot: -0.12534081, w_bar_dot: 0.16047689, l_dot: 149472.67411175 },
    // ... Mercury through Neptune, 8 entries total
    // These are published constants — copy from JPL tables
];
```

### Planet Rendering

Each planet rendered as the 1m cube OBJ (`assets/1m_cube.obj`) scaled to a fixed
angular size. Since even Jupiter (max ~0.02°) is sub-pixel at true angular size,
render all planets at an artificially inflated angular size of ~0.05° (roughly 3 pixels
at typical FOV). This makes them visible as tiny cubes — "wandering stars" that are
clearly synthetic, fitting the wireframe aesthetic.

```rust
const PLANET_ANGULAR_SIZE_RAD: f64 = 0.000_873; // ~0.05° — visible as a few pixels
const PLANET_RENDER_DISTANCE: f64 = 80_000_000.0; // 80,000 km
```

Place at the correct direction from the camera, at PLANET_RENDER_DISTANCE, scaled
to subtend PLANET_ANGULAR_SIZE_RAD. Cubes instead of spheres immediately distinguish
planets from sun/moon.

---

## 6. Stars

### Catalog

Use the Hipparcos bright star catalog, filtered to the ~500 brightest stars (apparent
magnitude < ~4.0, which gives good naked-eye sky coverage). Store as a compiled Rust
array or load from a compact binary/CSV file at startup.

Required per star:
```rust
pub struct StarEntry {
    pub hip_id: u32,              // Hipparcos catalog number
    pub name: Option<&'static str>, // common name, if any
    pub ra_deg: f64,              // right ascension J2000 (degrees)
    pub dec_deg: f64,             // declination J2000 (degrees)
    pub mag: f32,                 // apparent visual magnitude
}
```

### Named Stars (must be present)

These seven plus other bright navigational stars:

| Name            | HIP     | RA (deg)  | Dec (deg)   | Mag   |
|-----------------|---------|-----------|-------------|-------|
| Sirius          | 32349   | 101.287   | -16.716     | -1.46 |
| Canopus         | 30438   | 95.988    | -52.696     | -0.74 |
| Alpha Centauri  | 71683   | 219.902   | -60.834     | -0.27 |
| Arcturus        | 69673   | 213.915   | +19.182     | -0.05 |
| Vega            | 91262   | 279.235   | +38.784     | +0.03 |
| Capella         | 24608   | 79.172    | +45.998     | +0.08 |
| Rigel           | 24436   | 78.634    | -8.202      | +0.13 |

Include Polaris (HIP 11767, mag +1.98) as well — it's not bright enough for the
top-7 but it's the most important navigational star.

### Star Catalog File

Ship as `assets/stars/hipparcos_bright.csv`:
```
hip_id,ra_deg,dec_deg,mag,name
32349,101.287,-16.716,-1.46,Sirius
30438,95.988,-52.696,-0.74,Canopus
...
```

Or embed directly in Rust as a const array for zero-dependency loading.

### Star Positions → ECEF

Stars are infinitely far away (effectively). Their positions are fixed in J2000
equatorial frame (RA/Dec). To render:

1. Convert RA/Dec to a unit vector in J2000 equatorial (ECI):
   ```rust
   let ra_rad = ra_deg.to_radians();
   let dec_rad = dec_deg.to_radians();
   let eci_dir = DVec3::new(
       dec_rad.cos() * ra_rad.cos(),
       dec_rad.cos() * ra_rad.sin(),
       dec_rad.sin(),
   );
   ```

2. Rotate from ECI to ECEF using GMST (same transform as sun/moon):
   ```rust
   let ecef_dir = eci_to_ecef(eci_dir, gmst_rad);
   ```

3. Place at a large fixed distance from the camera:
   ```rust
   const STAR_RENDER_DISTANCE: f64 = 90_000_000.0; // 90,000 km
   let star_pos = camera_ecef + ecef_dir * STAR_RENDER_DISTANCE;
   ```

### Star Rendering

Each star is the 1m cube OBJ, placed at `STAR_RENDER_DISTANCE` in the correct direction,
scaled to a fixed angular size. Stars should be tiny — just 1-2 pixels. Brighter stars
could be slightly larger:

```rust
fn star_angular_size(mag: f32) -> f64 {
    // Brighter = lower magnitude = larger rendered size
    // mag -1.5 (Sirius) → ~0.03° (2 pixels)
    // mag +4.0 (faintest) → ~0.01° (barely 1 pixel)
    let base = 0.000_35; // ~0.02° baseline
    let scale = 10.0_f64.powf((-mag as f64) / 5.0); // brighter = larger
    (base * scale).clamp(0.000_17, 0.000_52) // 0.01° to 0.03°
}
```

### Star Visibility

Stars should only be visible when looking away from the sun (roughly). During daytime
at low altitude, real stars aren't visible. Simple approximation:

- Compute sun altitude above the observer's horizon (from the sun position)
- If sun altitude > 0° (daytime): don't render stars
- If sun altitude < -6° (civil twilight past): render all stars
- Between 0° and -6°: fade stars in (reduce rendered count by magnitude cutoff)

At orbital altitude, stars are always visible (no atmospheric scattering). Use altitude
as a secondary factor:

```rust
fn stars_visible(sun_altitude_deg: f64, observer_altitude_m: f64) -> bool {
    observer_altitude_m > 100_000.0  // above atmosphere: always visible
    || sun_altitude_deg < -6.0       // civil twilight: stars visible
}
```

---

## 7. CelestialEngine

```rust
pub struct CelestialEngine {
    pub clock: SimClock,
    // Cached positions (ECEF), updated at ~1 Hz
    pub sun_ecef: DVec3,
    pub moon_ecef: DVec3,
    pub planet_ecef: [DVec3; 8],     // Mercury through Neptune
    pub star_dirs_ecef: Vec<DVec3>,  // unit direction vectors in ECEF
    // Raw catalog
    star_catalog: Vec<StarEntry>,
    planet_elements: Vec<PlanetElements>,
    // Update timing
    last_update_jd: f64,
    update_interval_jd: f64,  // ~1 second = 1/86400 JD
    // Cached derived quantities
    pub sun_altitude_deg: f64,  // sun altitude above observer horizon
    pub moon_phase: f64,        // 0.0 = new, 0.5 = full, 1.0 = new
    pub gmst_rad: f64,
}

impl CelestialEngine {
    /// Called every frame. Advances clock, updates positions if interval elapsed.
    pub fn update(&mut self, dt: f64, observer_ecef: DVec3) {
        self.clock.advance(dt);

        let jd = self.clock.jd();
        if (jd - self.last_update_jd).abs() >= self.update_interval_jd {
            self.recompute(jd);
            self.last_update_jd = jd;
        }

        // Update observer-dependent quantities every frame (cheap)
        self.update_observer(observer_ecef);
    }

    fn recompute(&mut self, jd: f64) {
        let t = jd_to_t(jd);
        self.gmst_rad = gmst_deg(jd).to_radians();

        // Sun
        let sun = sun_position(jd);
        self.sun_ecef = eci_to_ecef(sun.eci, self.gmst_rad);

        // Moon
        let moon = moon_position(jd);
        self.moon_ecef = eci_to_ecef(moon.eci, self.gmst_rad);

        // Planets
        for (i, elements) in self.planet_elements.iter().enumerate() {
            let eci = planet_position(elements, t);
            self.planet_ecef[i] = eci_to_ecef(eci, self.gmst_rad);
        }

        // Stars (rotate all direction vectors from ECI to ECEF)
        self.star_dirs_ecef = self.star_catalog.iter().map(|star| {
            let ra = star.ra_deg.to_radians();
            let dec = star.dec_deg.to_radians();
            let eci = DVec3::new(
                dec.cos() * ra.cos(),
                dec.cos() * ra.sin(),
                dec.sin(),
            );
            eci_to_ecef(eci, self.gmst_rad)
        }).collect();
    }

    fn update_observer(&mut self, observer_ecef: DVec3) {
        // Sun altitude above horizon
        let lla = coords::ecef_to_lla(observer_ecef);
        let enu = coords::enu_frame_at(lla.lat, lla.lon, observer_ecef);
        let sun_dir = (self.sun_ecef - observer_ecef).normalize();
        let sun_enu = enu.ecef_to_enu(sun_dir);
        self.sun_altitude_deg = sun_enu.z.asin().to_degrees();

        // Moon phase
        let sun_dir_norm = (self.sun_ecef - observer_ecef).normalize();
        let moon_dir_norm = (self.moon_ecef - observer_ecef).normalize();
        let phase_angle = sun_dir_norm.dot(moon_dir_norm).clamp(-1.0, 1.0).acos();
        self.moon_phase = (1.0 + phase_angle.cos()) / 2.0;
    }
}
```

### Update Frequency

The `update_interval_jd` of ~1 second (1/86400 JD) means celestial positions recompute
once per second. Star rotations, planet positions, and sun/moon all update at this rate.
The observer-dependent quantities (sun altitude, moon phase) update every frame since
they depend on camera position and are cheap.

At 1 Hz, even the moon (which moves ~0.5°/hour = 0.00014°/second) shifts by an
imperceptible amount between updates. The sun moves even slower. Stars rotate at
~0.004°/second due to Earth rotation — also imperceptible at 1 Hz updates.

---

## 8. SceneObject Management

Each celestial body becomes one or more SceneObjects. Unlike the earth mesh (which
rebuilds vertices per-frame), celestial bodies use the angular-size rendering trick
and only need position/scale updates per frame.

### Object Count

- Sun: 1 SceneObject (dense icosphere)
- Moon: 1 SceneObject (moderate icosphere, LOD swap at close range)
- Planets: 8 SceneObjects (1m cube mesh, shared vertex/index buffers)
- Stars: ~500 SceneObjects (1m cube mesh, shared vertex/index buffers)

**Total: ~510 new SceneObjects.** This exceeds `MAX_OBJECTS = 128` in `renderer.rs`.

### MAX_OBJECTS Solution

Since we can't modify `renderer.rs`, we need to either:

1. **Increase MAX_OBJECTS** — this is a constant in `renderer.rs` that sizes the uniform
   buffer. Changing just this constant is a minimal renderer touch. Bump to 1024.

2. **Batch stars into a single SceneObject** — generate all ~500 star cubes as a single
   merged mesh (concatenate all vertices/indices with per-star offsets). This is one
   SceneObject with ~6000 vertices (500 cubes × 8 vertices + 500 × 12 tris). Rebuild
   vertex positions per-frame (camera-relative) since each star is in a different direction.
   This is the cleanest approach.

3. **Render stars separately** — add a simple point-rendering pass. This requires
   renderer changes.

**Recommended: option 2.** Merge all stars into one SceneObject, all planets into another,
keep sun and moon as individual objects. Total new SceneObjects: 4 (sun, moon, planets_merged,
stars_merged). Well within MAX_OBJECTS.

The merged mesh approach:
```rust
fn build_star_mesh(
    cube_mesh: &MeshData,    // the 1m cube template
    star_dirs_ecef: &[DVec3],
    star_sizes: &[f64],       // angular sizes per star
    camera_ecef: DVec3,
) -> MeshData {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for (i, (dir, size)) in star_dirs_ecef.iter().zip(star_sizes).enumerate() {
        let pos = camera_ecef + *dir * STAR_RENDER_DISTANCE;
        let rel = pos - camera_ecef;  // = dir * STAR_RENDER_DISTANCE
        let scale = STAR_RENDER_DISTANCE * (size / 2.0).tan();

        let base_idx = vertices.len() as u32;
        for v in &cube_mesh.vertices {
            vertices.push(Vertex {
                position: [
                    (rel.x as f32) + v.position[0] * scale as f32,
                    (rel.y as f32) + v.position[1] * scale as f32,
                    (rel.z as f32) + v.position[2] * scale as f32,
                ],
                normal: v.normal,
            });
        }
        for idx in &cube_mesh.indices {
            indices.push(base_idx + idx);
        }
    }

    MeshData { vertices, indices }
}
```

Rebuild this merged mesh once per second (when celestial positions update). Between
updates, the stars drift by ~0.004° which is invisible.

---

## 9. Day/Night Considerations

The sun position enables basic day/night awareness:

- **sun_altitude_deg > 0**: Daytime. FSBLUE background is appropriate. Stars hidden.
- **sun_altitude_deg < -6**: Night. Stars visible. Background is still FSBLUE (no sky
  color change yet — that's a shader modification for future work).
- **At altitude > 100 km**: Stars always visible (space).

For v1, the only visual effect of day/night is star visibility. The FSBLUE background
stays constant. Actual sky coloring, sunset gradients, atmospheric scattering — all
future shader work. But the celestial engine provides all the data needed for those
effects when the time comes.

---

## 10. Gravity Model for Lunar Missions

While not strictly a "celestial rendering" concern, the navigable moon implies the
physics engine needs to handle lunar gravity. When the player is closer to the moon
than to the earth (roughly the L1 point, ~326,000 km from Earth, ~58,000 km from Moon),
lunar gravity dominates:

```rust
// In physics step:
let r_earth = (pos_ecef - DVec3::ZERO).length(); // distance to Earth center
let r_moon = (pos_ecef - moon_ecef).length();     // distance to Moon center

const GM_EARTH: f64 = 3.986_004_418e14;  // m³/s²
const GM_MOON: f64 = 4.902_800e12;        // m³/s²

let a_earth = -GM_EARTH / (r_earth * r_earth * r_earth) * pos_ecef;
let a_moon = -GM_MOON / (r_moon * r_moon * r_moon) * (pos_ecef - moon_ecef);
let gravity_ecef = a_earth + a_moon;
```

Both gravitational accelerations are always computed — near Earth the moon's contribution
is negligible, near the moon the Earth's contribution is small but non-zero (which is
correct — that's how real orbital mechanics works). This is a physics.rs change, noted
here for completeness.

---

## Dependencies

```toml
chrono = "0.4"    # for --epoch parsing (ISO 8601 → Unix timestamp)
```

Or parse ISO 8601 manually to avoid the dependency. `chrono` is lightweight though.

The star catalog is either embedded as a const array or loaded from a CSV (no new
dependency needed — manual CSV parsing for a simple fixed-format file is trivial).

---

## Do NOT

- Modify `renderer.rs` or shaders (except bumping MAX_OBJECTS if approach 2 is insufficient)
- Use a full ephemeris library (JPL DE440, SPICE, etc.) — analytical models are sufficient
- Implement atmospheric scattering or sky color (future shader work)
- Implement star twinkling or magnitude-based color (future)
- Compute precession/nutation corrections (sub-degree effects, irrelevant for visual work)
- Use f32 for any celestial position math — everything is f64 until the final vertex output

---

## Verification

After implementation:
- At SFO at 3 PM local time in January: sun should be low in the western sky
- At midnight: moon visible (if above horizon for current phase), stars visible
- Moon phase should roughly match reality for the current date
- Stars should rotate east to west ~15°/hour (Earth rotation)
- Polaris should be within ~1° of true north at the observer's latitude elevation
- The seven named stars (Sirius, Canopus, Alpha Centauri, Arcturus, Vega, Capella, Rigel)
  should be identifiable in their correct constellations
- Planets should be visible as tiny cubes in approximately correct positions along the ecliptic
- Flying toward the moon: it grows from a small wireframe sphere to a large globe filling the view
- In lunar orbit: moon surface visible as wireframe sphere below, earth visible as wireframe globe in the sky
- Performance: <1ms per celestial update (once per second), negligible per-frame cost