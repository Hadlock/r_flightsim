# CLAUDE.md — Fix Object Placement (Teapots + Origin Parsing)

## Problem
1. Teapots are hardcoded relative to `ref_pos` (aircraft start), not using `# origin:` tags. They appear glued to the plane's starting position rather than fixed in the world.
2. The origin parser likely fails on the degree symbol. The OBJ files contain `°` (UTF-8 U+00B0, 2 bytes: 0xC2 0xB0), but the parser searches for `Â°` (the mojibake version where UTF-8 bytes get misinterpreted as Latin-1). This means ALL DMS-format origins silently fail to parse, so no cubes or pyramids load.
3. Only objects whose origins are in decimal format (like `37.795200, -122.402800`) would parse successfully.

## Fix 1: Make Teapot Use `# origin:` Tag

The teapot.obj already has `# origin: 37°36'52.2"N 122°21'32.2"W`. Remove the hardcoded teapot placement from `load_scene()` entirely. Let it be auto-loaded like all other landmarks.

Remove the teapot from the `skip` list in `load_scene()`:
```rust
let skip = [
    "14082_WWII_Plane_Japan_Kawasaki_Ki-61_v1_L2.obj",
];
```

Delete the entire teapot hardcoded block (the `teapot_mesh`, `teapot_scale`, `teapot_rotation`, and the `for i in 0..10` loop).

The teapot OBJ uses Y-up convention (like most modeling software), NOT ENU Z-up. We'll handle this below.

## Fix 2: Fix Degree Symbol Parsing

In `scene.rs`, the `parse_dms_component` function searches for `'Â°'` which is wrong. It needs to search for `'°'` (Unicode U+00B0).

**Replace the degree symbol handling in `parse_dms_component`:**

The issue is likely that the source code itself has the wrong character. The fix:

```rust
fn parse_dms_component(s: &str) -> Option<f64> {
    let s = s.trim();
    let direction = s.chars().last()?;
    if !matches!(direction, 'N' | 'S' | 'E' | 'W') {
        return None;
    }
    let s = &s[..s.len() - direction.len_utf8()];

    // Find degree symbol: handle both ° (U+00B0) and any multi-byte variant
    let deg_end = s.find('\u{00B0}')?;  // Unicode degree sign
    let deg_symbol_len = '\u{00B0}'.len_utf8(); // 2 bytes in UTF-8
    let min_end = s.find('\'')?;
    let sec_end = s.find('"')?;

    let degrees: f64 = s[..deg_end].parse().ok()?;
    let minutes: f64 = s[deg_end + deg_symbol_len..min_end].parse().ok()?;
    let seconds: f64 = s[min_end + 1..sec_end].parse().ok()?;

    let mut value = degrees + minutes / 60.0 + seconds / 3600.0;
    if direction == 'S' || direction == 'W' {
        value = -value;
    }
    Some(value)
}
```

Also fix `parse_dms` which checks for `'Â°'`:
```rust
fn parse_dms(s: &str) -> Option<LLA> {
    if !s.contains('\u{00B0}') {  // Unicode degree sign
        return None;
    }
    // ... rest unchanged
}
```

## Fix 3: Handle Y-up vs Z-up OBJ Convention

The OBJ files fall into two categories:
- **Custom landmarks** (cubes, pyramids): Created by us in ENU convention (X=east, Y=north, Z=up). These get `enu_to_ecef_quat` rotation only.
- **Downloaded models** (teapot, and potentially others): Standard Y-up convention (X=right, Y=up, Z=forward or similar). These need a Y-up→Z-up rotation BEFORE the ENU→ECEF rotation.

Add a second comment tag to distinguish. Use `# convention:` with values `enu` or `yup`:

For our custom OBJs (cubes, pyramids), add:
```
# convention: enu
```

For the teapot (Y-up), add:
```
# convention: yup
```

If no convention tag is present, default to `enu` (since most of our objects use it).

**Parse the convention tag alongside origin:**

```rust
enum ObjConvention {
    Enu,  // X=east, Y=north, Z=up (our custom landmarks)
    Yup,  // standard modeling Y-up
}

fn parse_obj_metadata(path: &Path) -> (Option<LLA>, ObjConvention) {
    let mut origin = None;
    let mut convention = ObjConvention::Enu;

    if let Ok(file) = fs::File::open(path) {
        let reader = BufReader::new(file);
        for line in reader.lines().take(15).flatten() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("# origin:") {
                let rest = rest.trim();
                origin = parse_dms(rest).or_else(|| parse_decimal(rest));
            }
            if let Some(rest) = trimmed.strip_prefix("# convention:") {
                match rest.trim() {
                    "yup" => convention = ObjConvention::Yup,
                    _ => convention = ObjConvention::Enu,
                }
            }
        }
    }

    (origin, convention)
}
```

**Compute rotation based on convention:**

```rust
fn object_rotation(lla: &LLA, convention: &ObjConvention) -> Quat {
    let enu_quat = enu_to_ecef_quat(lla.lat, lla.lon);
    match convention {
        ObjConvention::Enu => enu_quat,
        ObjConvention::Yup => {
            // Y-up to Z-up: rotate -90° around X (Y becomes Z, Z becomes -Y)
            let y_to_z = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            enu_quat * y_to_z
        }
    }
}
```

## Fix 4: Update OBJ Files with Convention Tags

Add `# convention: enu` to all our custom ENU objects:
- `1m_cube.obj`
- `10m_cube.obj` 
- `30m_cube.obj`
- `pyramid_giza.obj`
- `pyramid_transamerica.obj`
- `pyramid_mountain.obj`

Add `# convention: yup` to Y-up models:
- `teapot.obj`

## Fix 5: Simplify `load_scene()`

After these changes, `load_scene()` becomes much simpler — just auto-load everything:

```rust
pub fn load_scene(device: &wgpu::Device) -> Vec<SceneObject> {
    let mut objects = Vec::new();
    let mut id = 10u32;

    let skip = [
        "14082_WWII_Plane_Japan_Kawasaki_Ki-61_v1_L2.obj",
    ];

    let mut entries: Vec<_> = fs::read_dir("assets")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.path().extension().map_or(false, |ext| ext == "obj")
                && !skip.iter().any(|s| e.file_name().to_string_lossy() == *s)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let (origin, convention) = parse_obj_metadata(&path);
        if let Some(lla) = origin {
            let ecef_pos = coords::lla_to_ecef(&lla);
            let rotation = object_rotation(&lla, &convention);
            let mesh = obj_loader::load_obj(&path);
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            log::info!(
                "Loaded '{}' at ({:.4}°, {:.4}°) convention={:?}",
                name, lla.lat.to_degrees(), lla.lon.to_degrees(), convention
            );
            objects.push(spawn(device, &mesh, &name, ecef_pos, rotation, 1.0, id));
            id += 1;
        }
    }

    objects
}
```

**Update `main.rs`** — `load_scene` no longer needs `ref_pos` or `enu` parameters:
```rust
let mut objects = scene::load_scene(&device);
```

Remove the `ref_pos` and `enu` variables from `resumed()` if they're only used for `load_scene`.

## Fix 6: Add Debug Logging

Add a log line if an OBJ has no origin (helps catch parse failures):
```rust
if origin.is_none() {
    log::warn!("No valid # origin: found in {:?}, skipping", path);
}
```

## Fix 7: Verify Teapot Scale

The teapot vertices range from about -3 to +6 units. At scale 1.0, that's a 6-9m teapot. That's fine for visibility. If it's too big or too small, add a `# scale:` tag later, but for now scale=1.0 for all auto-loaded objects.

## Summary of Changes

### Files modified:
- **`scene.rs`** — fix degree symbol, remove hardcoded teapots, add convention parsing, simplify load_scene signature
- **`main.rs`** — update load_scene call (remove ref_pos/enu args)
- **`assets/teapot.obj`** — add `# convention: yup`
- **`assets/1m_cube.obj`** — add `# convention: enu`
- **`assets/10m_cube.obj`** — add `# convention: enu`
- **`assets/30m_cube.obj`** — add `# convention: enu`
- **`assets/pyramid_giza.obj`** — add `# convention: enu`
- **`assets/pyramid_transamerica.obj`** — add `# convention: enu`
- **`assets/pyramid_mountain.obj`** — add `# convention: enu`

### Files NOT modified:
- `physics.rs`, `coords.rs`, `camera.rs`, `renderer.rs`, `obj_loader.rs`, `sim.rs`, `shaders/*`

## Test
1. `cargo run --release` with `RUST_LOG=info`
2. Check console for "Loaded 'pyramid_giza' at (37.6160°, -122.3880°)" etc. — if you see these, parsing works
3. If you see "No valid # origin: found" for cubes/pyramids, the degree symbol is still wrong
4. Teapot should be fixed at its origin coordinates, NOT moving with the plane
5. All cubes and pyramids should be visible at their world positions
6. Rolling down 28R, cubes on 28L centerline visible to the left, teapot on taxiway to the right
7. After takeoff, pyramids visible at distance