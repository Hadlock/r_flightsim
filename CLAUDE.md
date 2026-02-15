# CLAUDE.md — shaderflight: Codebase Consolidation

## Overview

This is a structural cleanup pass. No new features. The goal is to reduce duplication,
extract main.rs into manageable pieces, and make the codebase easier to extend for
shuttle missions, celestial rendering, and future systems.

Every change here should be behavior-preserving. The simulator should run identically
before and after. `cargo test` should pass. No visual changes.

---

## 1. Extract `enu_to_ecef_quat` into coords.rs

### Problem

The same function exists in three places with identical logic:

- `scene.rs:260` — `fn enu_to_ecef_quat(lat_rad, lon_rad) -> Quat`
- `airport_markers.rs:163` — `fn enu_to_ecef_quat(lat_rad, lon_rad) -> Quat`
- `ai_traffic.rs:70` — `fn compute_orientation(lla, heading, bank) -> DQuat` (builds
  ENU frame then constructs body axes — uses the same ENU-to-ECEF rotation internally)

### Fix

Add to `coords.rs`:

```rust
/// ENU-to-ECEF rotation quaternion at a given geodetic position.
/// Returns f32 Quat suitable for SceneObject rotation.
pub fn enu_to_ecef_quat(lat_rad: f64, lon_rad: f64) -> glam::Quat {
    let enu = enu_frame_at(lat_rad, lon_rad, DVec3::ZERO);
    let mat = glam::DMat3::from_cols(enu.east, enu.north, enu.up);
    let dq = glam::DQuat::from_mat3(&mat);
    glam::Quat::from_xyzw(dq.x as f32, dq.y as f32, dq.z as f32, dq.w as f32)
}

/// ENU-to-ECEF rotation as f64 DQuat (for physics/AI traffic).
pub fn enu_to_ecef_dquat(lat_rad: f64, lon_rad: f64) -> glam::DQuat {
    let enu = enu_frame_at(lat_rad, lon_rad, DVec3::ZERO);
    let mat = glam::DMat3::from_cols(enu.east, enu.north, enu.up);
    glam::DQuat::from_mat3(&mat)
}
```

Then:
- `scene.rs`: delete local `enu_to_ecef_quat`, call `coords::enu_to_ecef_quat`
- `airport_markers.rs`: delete local `enu_to_ecef_quat`, call `coords::enu_to_ecef_quat`
- `ai_traffic.rs`: `compute_orientation` can use `coords::enu_to_ecef_dquat` internally
  for the base ENU rotation, then apply heading/bank on top. The function itself stays
  in `ai_traffic.rs` since it adds heading+bank logic, but the underlying ENU rotation
  comes from coords.

---

## 2. Cache Aircraft Profiles on App

### Problem

`aircraft_profile::load_all_profiles()` is called three times:
- `init_menu()` — scans filesystem, parses all YAML
- `init_flying()` — scans filesystem, parses all YAML again
- `transition_to_menu()` — scans filesystem, parses all YAML again

Also called in the `ReturnToMenu` handler to find the previous selection index:
```rust
FlyingAction::ReturnToMenu => {
    let profiles = aircraft_profile::load_all_profiles(Path::new("assets/planes"));
    let prev_idx = profiles.iter().position(|p| p.name == flying.aircraft_name);
    ...
}
```

### Fix

Add `cached_profiles: Option<Vec<AircraftProfile>>` to the `App` struct, alongside
the existing `parsed_airports` pattern.

Load once in `init_menu()` (or on first use). Pass by reference everywhere else:

```rust
fn ensure_profiles(&mut self) -> &[AircraftProfile] {
    if self.cached_profiles.is_none() {
        self.cached_profiles = Some(
            aircraft_profile::load_all_profiles(Path::new("assets/planes"))
        );
    }
    self.cached_profiles.as_ref().unwrap()
}
```

- `init_menu()`: use `self.ensure_profiles()`, clone into MenuState
- `init_flying(slug)`: use `self.ensure_profiles()` to find profile
- `transition_to_menu(prev_idx)`: caller already knows the index or name, pass it
  through — no re-scan needed
- `ReturnToMenu`: find prev_idx from `flying.aircraft_name` against cached profiles
  before dropping flying state

---

## 3. Extract FlyingState into src/flying.rs

### Problem

main.rs is 1522 lines. The flying state initialization (`init_flying`) is ~290 lines.
The flying event handler + render loop (`handle_flying_event`) is ~330 lines. The
telemetry publisher is ~70 lines. The radio overlay is ~70 lines. Together these
account for ~760 lines of main.rs that are all flying-specific.

### Fix

Create `src/flying.rs` containing:

```rust
pub struct FlyingState {
    // All existing fields from the FlyingState in main.rs
    pub renderer: Renderer,
    pub camera: Camera,
    pub objects: Vec<SceneObject>,
    // ... etc
}

pub enum FlyingAction {
    None,
    ReturnToMenu,
    ReconfigureSurface,
    UpdateTelemetry,
}

impl FlyingState {
    /// Initialize the flying state. Moved from App::init_flying().
    pub fn new(
        gpu: &GpuContext,
        profile: &AircraftProfile,
        parsed_airports: &mut Option<airport_gen::ParsedAirports>,
        epoch_unix: Option<f64>,
        no_tts: bool,
    ) -> Self { ... }

    /// Handle a window event. Moved from App::handle_flying_event().
    pub fn handle_event(
        &mut self,
        gpu: &GpuContext,
        event: &WindowEvent,
    ) -> FlyingAction { ... }

    /// Collect telemetry snapshot. Moved from App::publish_flying_telemetry().
    pub fn telemetry_snapshot(&self) -> telemetry::Telemetry { ... }
}
```

main.rs then becomes:

```rust
// In window_event, Flying branch:
GameState::Flying(mut flying) => {
    let gpu = self.gpu.as_ref().unwrap();
    let action = flying.handle_event(gpu, &event);
    match action {
        FlyingAction::ReturnToMenu => { ... }
        FlyingAction::UpdateTelemetry => {
            *self.shared_telemetry.lock().unwrap() = flying.telemetry_snapshot();
        }
        ...
    }
    self.game_state = Some(GameState::Flying(flying));
}
```

This cuts main.rs by ~700 lines and keeps the flying state self-contained.

### What stays in main.rs

- `App` struct and `ApplicationHandler` impl
- `GpuContext` struct and GPU initialization (`resumed`)
- `GameState` enum and state transitions
- `MenuStateWrapper` and menu event handling (small enough to stay)
- `main()` function

Target: main.rs should be ~600-700 lines after this extraction.

---

## 4. Extract egui Boilerplate into a Helper

### Problem

The egui initialization sequence (Context + State + Renderer) is repeated three times:

- `init_menu()` lines 157–172
- `init_flying()` lines 423–445
- `transition_to_menu()` lines 515–530

The egui render pass boilerplate (tessellate → update textures → create encoder →
begin render pass → render → submit → free textures) is repeated twice:

- Flying state: lines 798–871
- Menu state: similar block in the menu RedrawRequested handler

### Fix

Create a small helper in main.rs (or a new `src/egui_helpers.rs` if preferred):

```rust
struct EguiContext {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}

impl EguiContext {
    fn new(gpu: &GpuContext) -> Self {
        let ctx = egui::Context::default();
        let state = egui_winit::State::new(
            ctx.clone(), ctx.viewport_id(), &gpu.window, None, None, None,
        );
        let renderer = egui_wgpu::Renderer::new(
            &gpu.device, gpu.surface_format, None, 1, false,
        );
        Self { ctx, state, renderer }
    }

    /// Run egui, tessellate, render to the given surface view.
    /// `ui_fn` is the closure that builds the egui UI.
    fn render_to_surface(
        &mut self,
        gpu: &GpuContext,
        surface_view: &wgpu::TextureView,
        ui_fn: impl FnOnce(&egui::Context),
    ) {
        let raw_input = self.state.take_egui_input(&gpu.window);
        let full_output = self.ctx.run(raw_input, ui_fn);
        self.state.handle_platform_output(&gpu.window, full_output.platform_output);

        let clipped = self.ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        let screen_desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [gpu.config.width, gpu.config.height],
            pixels_per_point: full_output.pixels_per_point,
        };

        for (id, delta) in &full_output.textures_delta.set {
            self.renderer.update_texture(&gpu.device, &gpu.queue, *id, delta);
        }

        let mut encoder = gpu.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("egui encoder") },
        );
        self.renderer.update_buffers(
            &gpu.device, &gpu.queue, &mut encoder, &clipped, &screen_desc,
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            }).forget_lifetime();
            self.renderer.render(&mut pass, &clipped, &screen_desc);
        }
        gpu.queue.submit(std::iter::once(encoder.finish()));

        for id in &full_output.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }
}
```

Replace the three init sites with `EguiContext::new(gpu)`. Replace the two render
sites with `egui_ctx.render_to_surface(gpu, &surface_view, |ctx| { ... })`.

The flying egui context needs transparent styling applied after creation — do that
in `FlyingState::new()` after calling `EguiContext::new()`.

---

## 5. Move Radio Overlay to atc/overlay.rs

### Problem

`draw_radio_overlay()` is a free function in main.rs (lines 1361–1430) that draws
ATC-specific UI. It depends on `atc::types::RadioMessage` and `atc::types::Speaker`.
It has no dependency on main.rs internals.

### Fix

Move to `src/atc/overlay.rs`:

```rust
// src/atc/overlay.rs
use super::types::{RadioMessage, Speaker};

pub fn draw_radio_overlay(
    ctx: &egui::Context,
    messages: &[&RadioMessage],
    com1_freq: f32,
) { ... }
```

Add `pub mod overlay;` to `src/atc/mod.rs`.

Call from flying state as `atc::overlay::draw_radio_overlay(...)`.

---

## 6. Consolidate Telemetry Derivation

### Problem

`publish_flying_telemetry` in main.rs (lines 1433–1501) recomputes heading, pitch,
bank, airspeed, and alpha from the aircraft state. These are standard flight instrument
derivations that could be useful elsewhere (HUD, autopilot, guidance).

### Fix

Add a `FlightInstruments` struct and a derivation method to `physics.rs` (or a new
`src/instruments.rs`):

```rust
pub struct FlightInstruments {
    pub heading_deg: f64,
    pub pitch_deg: f64,
    pub bank_deg: f64,
    pub airspeed_kts: f64,
    pub groundspeed_kts: f64,
    pub vertical_speed_fpm: f64,
    pub altitude_msl_ft: f64,
    pub altitude_agl_ft: f64,
    pub alpha_deg: f64,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub on_ground: bool,
}

impl FlightInstruments {
    pub fn from_aircraft(aircraft: &RigidBody) -> Self {
        let nose_ecef = aircraft.orientation * DVec3::X;
        let nose_enu = aircraft.enu_frame.ecef_to_enu(nose_ecef);
        let hdg = nose_enu.x.atan2(nose_enu.y).to_degrees();

        let right_ecef = aircraft.orientation * DVec3::Y;
        let right_enu = aircraft.enu_frame.ecef_to_enu(right_ecef);

        let vel_body = aircraft.orientation.conjugate() * aircraft.vel_ecef;

        Self {
            heading_deg: if hdg < 0.0 { hdg + 360.0 } else { hdg },
            pitch_deg: nose_enu.z.asin().to_degrees(),
            bank_deg: right_enu.z.asin().to_degrees(),
            airspeed_kts: vel_body.length() * 1.94384,
            groundspeed_kts: aircraft.groundspeed * 1.94384,
            vertical_speed_fpm: aircraft.vertical_speed * 196.85,
            altitude_msl_ft: aircraft.lla.alt * 3.28084,
            altitude_agl_ft: aircraft.agl * 3.28084,
            alpha_deg: if vel_body.x.abs() > 0.1 {
                vel_body.z.atan2(vel_body.x).to_degrees()
            } else {
                0.0
            },
            latitude_deg: aircraft.lla.lat.to_degrees(),
            longitude_deg: aircraft.lla.lon.to_degrees(),
            on_ground: aircraft.on_ground,
        }
    }
}
```

Then `publish_flying_telemetry` becomes:

```rust
fn telemetry_snapshot(&self) -> telemetry::Telemetry {
    let instruments = FlightInstruments::from_aircraft(&self.sim_runner.sim.aircraft);
    let mut t = telemetry::Telemetry::default();
    t.heading_deg = instruments.heading_deg;
    t.pitch_deg = instruments.pitch_deg;
    // ... direct field copy, no re-derivation
    t
}
```

---

## 7. Unify RadioLogEntry

### Problem

`RadioLogEntry` is defined in two places:

- `atc/types.rs` — `pub struct RadioLogEntry { frequency, speaker, text }` (richer type)
- `telemetry.rs` — `pub struct RadioLogEntry { frequency, speaker, text }` (identical fields)

The telemetry version exists because the ratatui dashboard can't depend on the full
ATC types. But the fields are the same.

### Fix

Delete `RadioLogEntry` from `telemetry.rs`. Re-export or use `atc::types::RadioLogEntry`
in the `Telemetry` struct:

```rust
// telemetry.rs
pub struct Telemetry {
    // ...
    pub radio_log: Vec<crate::atc::types::RadioLogEntry>,
}
```

If there's a concern about the telemetry module depending on atc, the alternative is
to define `RadioLogEntry` in telemetry.rs and have atc convert to it — but since both
modules are in the same crate, the direct dependency is fine and simpler.

---

## 8. Move Shared Constants to constants.rs

### Problem

`GM_EARTH` is defined in `physics.rs` (line 583) and will be needed by the celestial
module for lunar gravity, Kepler solving, etc. Unit conversion factors (3.28084 for
m→ft, 1.94384 for m/s→kts, 196.85 for m/s→fpm) are scattered as magic numbers.

### Fix

Create `src/constants.rs`:

```rust
// Gravitational parameters (m³/s²)
pub const GM_EARTH: f64 = 3.986_004_418e14;
pub const GM_MOON: f64 = 4.902_800e12;
pub const GM_SUN: f64 = 1.327_124_4e20;

// WGS-84 (re-exported from coords for convenience, or just reference coords)
pub const R_EARTH_EQUATORIAL: f64 = 6_378_137.0;

// Unit conversions
pub const M_TO_FT: f64 = 3.28084;
pub const MPS_TO_KTS: f64 = 1.94384;
pub const MPS_TO_FPM: f64 = 196.85;
pub const FT_TO_M: f64 = 0.3048;
pub const KM_TO_M: f64 = 1000.0;
pub const AU_TO_M: f64 = 149_597_870_700.0;
pub const NM_TO_M: f64 = 1852.0;
```

Replace magic numbers across the codebase with these constants. This is a
find-and-replace pass — no logic changes.

---

## 9. Object Index Bookkeeping (Advisory — Lower Priority)

### Problem

`FlyingState` tracks object indices with individual fields:

```rust
aircraft_idx: usize,
earth_idx: usize,
celestial_indices: [usize; 5],
marker_base_idx: usize,
// + ai_traffic.scene_indices: Vec<usize>
```

Every new subsystem adds another index field. The culling code has a growing exclusion
list. This is manageable today but will get worse with more systems.

### Suggested Improvement (implement later, not now)

Add a category tag to SceneObject:

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ObjectCategory {
    Static,          // buildings, landmarks
    Aircraft,        // player aircraft
    AiTraffic,       // AI planes
    Earth,           // earth mesh
    Celestial,       // sun, moon, planets, stars
    AirportMarker,   // proximity markers
    Airport,         // generated airport geometry
}

pub struct SceneObject {
    // ... existing fields ...
    pub category: ObjectCategory,
}
```

Then culling becomes:

```rust
let should_cull = !matches!(obj.category,
    ObjectCategory::Earth | ObjectCategory::Aircraft |
    ObjectCategory::Celestial | ObjectCategory::AirportMarker
) && dist - radius > DISTANCE_CULL_M;
```

And finding objects by category doesn't require tracking indices at all. This is a
larger refactor — defer until it becomes painful, which will likely be when the shuttle
mission adds more object types.

---

## Execution Order

Do these in order — each step is independently testable:

1. **enu_to_ecef_quat → coords.rs** (5 min, compile + verify)
2. **Cache profiles on App** (15 min, touch init_menu/init_flying/transition_to_menu)
3. **Unify RadioLogEntry** (5 min, delete duplicate, update imports)
4. **Create constants.rs** (15 min, create file + find-replace magic numbers)
5. **Move draw_radio_overlay → atc/overlay.rs** (5 min, move + update imports)
6. **Add FlightInstruments** (15 min, new struct + refactor publish_flying_telemetry)
7. **Extract EguiContext helper** (20 min, new struct + replace 3 init sites + 2 render sites)
8. **Extract FlyingState → flying.rs** (45 min, largest refactor — move code, fix imports, test)

Total: ~2 hours. Steps 1–6 are quick wins. Steps 7–8 are the big payoff.

After each step, run `cargo build` and `cargo run --release -- -i` to verify no regressions.

---

## Do NOT

- Change any behavior, visuals, or physics
- Modify `renderer.rs` or shaders
- Add new features
- Change the public API of any module (only internal reorganization)
- Rename files except for new files being created
- Break the menu ↔ flying state transition