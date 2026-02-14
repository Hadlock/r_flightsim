# CLAUDE.md — shaderflight: Menu System, Aircraft Profiles & State Machine

## Project Context

shaderflight is a wgpu-based flight simulator with wireframe/edge-detection aesthetics.
It uses a 2-pass Sobel edge-detection pipeline rendering white 1px edges on a dark blue
background ("FSBLUE": `r: 0.10, g: 0.20, b: 0.30, a: 1.0`). All world-space math uses
WGS-84 ECEF with ENU local frames. Physics is 6-DOF RK4 at 120Hz.

The codebase is Rust. Key source files:
```
main.rs          – winit event loop, wgpu init, state management
renderer.rs      – 2-pass renderer: geometry → G-buffer → Sobel edge detection
scene.rs         – SceneObject type, OBJ loading with # origin: metadata
camera.rs        – Pilot camera (position set by sim, mouse controls head look)
coords.rs        – WGS-84 ECEF ↔ LLA ↔ ENU conversions
obj_loader.rs    – OBJ mesh loading via tobj, smooth normal computation
physics.rs       – 6-DOF rigid body, RK4 integrator, ISA atmosphere, gear contact
sim.rs           – Fixed-timestep accumulator, input handling, telemetry
airport_gen.rs   – Procedural airport geometry from JSON
```

Aircraft OBJ models are in `assets/static_obj/planes/`.
Airport data is in `assets/airports/all_airports.json`.

## Task Overview

This work adds:
1. Aircraft profile system (YAML configs replacing hardcoded physics)
2. Application state machine (Menu → Flying)
3. Main menu with egui overlay (plane selection with spinning 3D preview)
4. CLI argument parsing with `clap`
5. ratatui telemetry dashboard in the terminal

---

## 1. Aircraft Profile System

### File Format

Each aircraft gets a YAML file in `assets/planes/<name>/profile.yaml`.
The OBJ file lives alongside it: `assets/planes/<name>/model.obj` (plus `.mtl` if present).

Example: `assets/planes/ki61_hien/profile.yaml`

```yaml
# Aircraft Profile — Ki-61 Hien
name: "Ki-61 Hien"
manufacturer: "Kawasaki"
country: "Japan"
year: 1942
description: "Imperial Japanese Army single-seat fighter, powered by a liquid-cooled Ha-40 inline engine."
category: "WWII Fighter"

model:
  obj: "model.obj"                # relative to this profile's directory

physics:
  mass: 2630.0                    # kg (loaded weight)
  inertia: [8000.0, 20000.0, 25000.0]  # Ixx, Iyy, Izz (kg·m²) — roll, pitch, yaw
  wing_area: 20.0                 # m²
  wing_span: 12.0                 # m (for display and future aero)
  max_thrust: 8500.0              # N
  cl0: 0.2
  cl_alpha: 5.0                   # per radian
  cd0: 0.025
  cd_alpha_sq: 0.04
  stall_alpha: 0.28               # radians (~16°)

# Gear contact points in body frame (X=fwd, Y=right, Z=down)
gear:
  - name: "left_main"
    position: [0.0, -1.5, 1.8]
  - name: "right_main"
    position: [0.0, 1.5, 1.8]
  - name: "tail_wheel"
    position: [-5.0, 0.0, 0.8]

stats:                            # Display stats for the menu
  wing_area: "20.0 m²"
  wing_span: "12.0 m"
  max_thrust: "8,500 N"
  mass: "2,630 kg"
  max_speed: "590 km/h"
  range: "1,100 km"
  ceiling: "11,400 m"
```

### Implementation

- Add `serde` + `serde_yaml` dependencies
- Create `src/aircraft_profile.rs`:
  - `AircraftProfile` struct with serde Deserialize
  - `load_all_profiles(base_path: &Path) -> Vec<AircraftProfile>` — scans `assets/planes/*/profile.yaml`
  - `AircraftProfile::to_aircraft_params(&self) -> AircraftParams` — converts to the existing physics struct
- The existing `AircraftParams::ki61()` constructor in `physics.rs` should remain as a fallback but the menu/profile system is the primary path
- Profile directory convention: `assets/planes/<slug>/` where slug is lowercase with underscores (e.g., `ki61_hien`, `cessna_172`, `concorde`, `b1_lancer`)

### Current hardcoded planes to create profiles for

Based on the menu mockup, create profile YAMLs for all of these (use best-available real data for physics params, reasonable estimates where exact data isn't public):

- Ki-61 Hien (already have physics, just needs YAML)
- Cessna 172
- Concorde
- OV-10A Bronco
- PZL M28 Skytruck
- King Air 200 (Beechcraft)
- A320neo (Airbus)
- Douglas DC-3
- B737-400
- DHC-7 Dash 7
- Piper PA28 Warrior
- B-1 Lancer

Each aircraft must have its OBJ in the corresponding directory. If an OBJ doesn't exist yet, the profile should still load — the menu can show the stats but display a "no model" placeholder or skip the 3D preview.

---

## 2. Application State Machine

### States

```rust
pub enum AppState {
    Menu(MenuState),
    Flying(FlyingState),
}
```

- **Menu**: egui draws the full UI, the Sobel pipeline renders only the spinning aircraft preview model. Physics is NOT running. Mouse cursor is visible and free.
- **Flying**: No egui. Full Sobel pipeline renders the scene. Physics runs at 120Hz. Mouse is grabbed for head look. This is the current behavior.

### Transitions

- `Menu → Flying`: User clicks "Fly Now" button. Selected aircraft profile is loaded into `AircraftParams`, sim is initialized at SFO 28L (lat: 37.613931, lon: -122.358089, heading: 280°, alt: 0.0). Scene objects are loaded. Cursor is grabbed.
- `Flying → Menu`: User presses Escape (or a dedicated key). Physics stops. Cursor is released. Return to menu preserving the previously selected aircraft.
- On launch (no `--instant` flag): Start in `Menu` state.
- On launch with `--instant` or `-ia`: Skip menu entirely, load Ki-61 (or last-used aircraft), go directly to `Flying` at SFO 28L. This is the current behavior, preserved for quick debug iteration.

### Implementation notes

- The `App` struct in `main.rs` currently holds everything in a flat `State`. Refactor to hold `AppState` enum. The wgpu device/queue/surface/window should live outside the state enum (they're shared). The state enum holds the game-specific state.
- The winit event loop dispatches to different handlers based on active state.
- When transitioning Menu→Flying, the scene objects, physics, camera etc. are initialized fresh.

---

## 3. Main Menu (egui + Sobel hybrid)

### Rendering architecture

- Use `egui-wgpu` and `egui-winit` crates for egui integration
- The menu screen has two layers:
  1. **Background + 3D preview**: The Sobel edge-detection pipeline renders the selected aircraft model rotating on the FSBLUE background. The model is centered in the right portion of the screen. Camera is fixed at a 3/4 overhead view — roughly 30° above the horizon, looking slightly down at the aircraft from a front-quarter angle. The model rotates counterclockwise (as seen from above) at approximately 5 RPM.
  2. **egui overlay**: Drawn on top of the 3D render. Semi-transparent panels for the UI chrome.

### Menu layout (from mockup)

```
┌──────────────────────────────────────────────────────────────┐
│ [Plane Select] [Airport Select] [Weather Select] [Settings]  │  ← tab bar
├──────────────┬───────────────────────────────────────────────┤
│              │                                               │
│  Ki-61 Hien  │         ╲                                     │
│  Cessna 172  │          ╲  ← 3D spinning model               │
│  Concorde    │          /   (Sobel wireframe, FSBLUE bg)     │
│  OV-10A      │         /                                     │
│  PZL M28     │                                               │
│  King Air    │      "Model Spins 5rpm"                       │
│  A320neo     │                                               │
│  DC-3        │  ┌─────────────────────────────────────────┐  │
│  B737-400    │  │ Wing Area: 20.0 m²    Wing Span: 12.0 m│  │
│  DHC-7       │  │ Max Thrust: 8,500 N   Mass: 2,630 kg   │  │
│  PA28        │  │ Max Speed: 590 km/h   Range: 1,100 km  │  │
│  B-1 Lancer  │  └─────────────────────────────────────────┘  │
│              │                                               │
│              │                              [ FLY NOW ]      │
└──────────────┴───────────────────────────────────────────────┘
```

### egui styling

- Overall color scheme: FSBLUE family. Panels use slightly lighter/darker variants of FSBLUE for depth.
- Text: white
- Active/selected items: white text on slightly lighter blue, or white border highlight
- "Fly Now" button: white background, FSBLUE text, bottom-right corner. Bold. Prominent.
- Tab bar across top: four tabs. Only "Plane Select" is functional for now. Other three tabs exist but show a centered "Coming Soon" message when clicked.
- Aircraft list: scrollable if needed. Selected item is highlighted. Clicking an item loads its preview model and stats.
- Stats panel: two-column layout below the 3D preview, showing values from the profile's `stats` section.
- Fonts: egui default is fine. If we want to match the wireframe aesthetic, consider a monospace font, but don't over-engineer this.

### 3D preview rendering details

- Load the selected aircraft's OBJ into a single SceneObject
- Position it at origin (no ECEF needed — this is a local preview scene)
- Camera: fixed position, looking at origin. Elevation ~30° above horizontal, azimuth rotating or fixed with model rotating. Choose whichever is simpler — rotating the model is probably easiest (just increment the Y rotation each frame).
- Rotation: counterclockwise when viewed from above = positive Y rotation in standard right-hand rule. 5 RPM = 30°/sec = 0.5236 rad/sec.
- The Sobel pipeline renders this into a region of the screen, and egui draws over/around it. The simplest approach: render the 3D preview to the full framebuffer, then draw egui panels with semi-transparent backgrounds on top/around it, leaving the center-right area clear so the model shows through.
- When no model OBJ exists for the selected aircraft, just render the FSBLUE background in the preview area (no model).

---

## 4. CLI Arguments (clap)

### Add dependency

```toml
clap = { version = "4", features = ["derive"] }
```

### Struct

```rust
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "shaderflight", about = "Wireframe flight simulator")]
pub struct Args {
    /// Instant action mode — skip menu, launch directly into flight
    #[arg(short = 'i', long = "instant")]
    pub instant: bool,

    /// Aircraft to load in instant mode (profile directory name)
    #[arg(short = 'a', long = "aircraft", default_value = "ki61_hien")]
    pub aircraft: String,

    /// Start in windowed mode (default is borderless fullscreen)
    #[arg(short = 'w', long = "windowed")]
    pub windowed: bool,
}
```

### Behavior

- `shaderflight` → opens menu
- `shaderflight --instant` or `shaderflight -i` → skip menu, Ki-61 at SFO (current behavior)
- `shaderflight -i -a concorde` → skip menu, Concorde at SFO
- `shaderflight -w` → windowed mode (useful for development)

---

## 5. ratatui Telemetry Dashboard

### Add dependencies

```toml
ratatui = "0.29"
crossterm = "0.28"
```

### Behavior

- Runs in the terminal that launched shaderflight, concurrently with the wgpu window
- Replaces the current `env_logger` / `log::info!` spam
- Shows a live-updating dashboard with key flight telemetry:
  - Airspeed (kts), Groundspeed (kts), Vertical speed (ft/min)
  - Altitude MSL (ft), AGL (ft)
  - Heading (°), Pitch (°), Bank (°)
  - Throttle (%), Alpha (°), G-load
  - Position (lat/lon)
  - FPS, physics tick rate
- Update rate: ~10Hz (no need to hammer the terminal at 60fps)
- Layout: compact, single screen, no scrolling. Something like a flight data recorder readout.
- Only active in `Flying` state. In `Menu` state, the terminal can show a simple "shaderflight — menu active" message or be blank.

### Implementation

- Spawn a thread or use an async task for the ratatui rendering loop
- The sim publishes telemetry to a shared struct (e.g., `Arc<Mutex<Telemetry>>` or `Arc<AtomicU64>` slots, or a crossbeam channel)
- ratatui reads from this shared state and redraws at its own pace
- On exit or state transition, ratatui cleans up the terminal properly (restore cursor, disable raw mode)
- `env_logger` can still be initialized for debug output, but normal operation should use the dashboard. Consider: ratatui in release mode, env_logger in debug mode, or a `--verbose` flag.

---

## 6. Migration Path for physics.rs

The existing `AircraftParams` struct in `physics.rs` has a `ki61()` constructor. The migration:

1. Keep `AircraftParams` as-is — it's the runtime physics struct
2. `AircraftProfile::to_aircraft_params()` creates one from the YAML data
3. Remove `AircraftParams::ki61()` once all profiles are in place (or keep it as a compile-time fallback with `#[cfg(debug_assertions)]`)
4. The gear contact points currently hardcoded in physics.rs move to the profile YAML

---

## Critical Constraints

- **Do NOT modify `renderer.rs` or the WGSL shaders** — the Sobel pipeline is done and working
- **Do NOT use f32 for world-space state** — ECEF positions are f64/DVec3/DQuat
- **Do NOT use Euler integration** — physics must remain RK4
- **Do NOT use any physics/game engine crate** (bevy, rapier, nalgebra)
- **Normalize quaternions after every integration step**
- The FSBLUE color is `(0.10, 0.20, 0.30, 1.0)` everywhere — wgpu clear color, egui backgrounds, etc.
- OBJ loading uses the existing `obj_loader.rs` and `scene.rs` pipeline — don't replace it
- All file I/O uses `expect("context message")`, never bare `unwrap()`

## Dependencies Summary (new)

```toml
# Add to [dependencies] in Cargo.toml
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
egui = "0.30"
egui-wgpu = "0.30"
egui-winit = "0.30"
ratatui = "0.29"
crossterm = "0.28"
```

Version numbers are approximate — use latest compatible versions. The egui crates must all be the same minor version.

## Build & Run

```bash
cargo run --release                     # opens menu
cargo run --release -- --instant        # instant action, Ki-61 at SFO
cargo run --release -- -i -a concorde   # instant action, Concorde at SFO
cargo run --release -- -w               # windowed mode with menu
```

## Controls (Flying State)

| Key        | Action                    |
|------------|---------------------------|
| ↑/↓        | Elevator (pitch)          |
| ←/→        | Aileron (roll)            |
| Z/X        | Rudder (yaw)              |
| Shift/Ctrl | Throttle up/down          |
| =/−        | Throttle up/down (alt)    |
| B          | Brakes                    |
| C          | Reset head look           |
| F11        | Toggle fullscreen         |
| Esc        | Return to menu            |
| Mouse      | Head look (when grabbed)  |