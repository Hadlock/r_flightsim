# CLAUDE.md — Fix Mouselook, Add Console Telemetry, Plan HUD

## Problem 1: Mouse Look is Broken

### What's happening
In `main.rs` RedrawRequested, the view matrix comes from `sim::aircraft_view_matrix(render_state.orientation)` which is purely the aircraft body orientation. The camera's yaw/pitch from mouse input (computed in `camera.rs`) is never incorporated into the view.

### Fix: Layer head rotation on top of aircraft orientation
The pilot can look around inside the cockpit. The view matrix should be:

```
view = head_rotation * aircraft_body_view
```

Where head_rotation is the mouse-look yaw/pitch relative to the aircraft's forward direction.

### Implementation

**In `sim.rs`**, change `aircraft_view_matrix` to accept head yaw and pitch:

```rust
/// Compute view matrix: aircraft orientation + pilot head look.
/// head_yaw: radians, 0 = looking forward, positive = look right
/// head_pitch: radians, 0 = level, positive = look up
pub fn aircraft_view_matrix(orientation: DQuat, head_yaw: f64, head_pitch: f64) -> Mat4 {
    // Aircraft body axes in ECEF
    let body_fwd = orientation * DVec3::X;     // nose direction
    let body_up = orientation * -DVec3::Z;     // body -Z = up (body Z = down)
    let body_right = orientation * DVec3::Y;   // body Y = right

    // Apply head yaw (rotate around body up axis)
    let yaw_rot = DQuat::from_axis_angle(body_up, -head_yaw);
    // Apply head pitch (rotate around body right axis)  
    let pitch_rot = DQuat::from_axis_angle(body_right, head_pitch);

    let look_dir = yaw_rot * pitch_rot * body_fwd;
    let up_dir = yaw_rot * pitch_rot * body_up;

    let view = DMat4::look_at_rh(DVec3::ZERO, look_dir, up_dir);
    let cols = view.to_cols_array();
    Mat4::from_cols_array(&cols.map(|v| v as f32))
}
```

**In `main.rs`**, pass camera yaw/pitch to the view matrix:

```rust
// In RedrawRequested:
let view = sim::aircraft_view_matrix(
    render_state.orientation,
    state.camera.yaw,
    state.camera.pitch,
);
```

**In `main.rs`**, route mouse input to `camera.mouse_move()` again. Currently mouse motion events go to `state.camera.mouse_move(dx, dy)` in `device_event` — this is correct and should already work since cursor_grabbed gates it. Verify this path is intact.

**In `camera.rs`**, the yaw/pitch should represent head-relative angles, not world-absolute. Reset them to 0.0 in `Camera::new()` (already the case). Optionally clamp yaw to ±150° so the pilot can't look behind through their own skull:

```rust
pub fn mouse_move(&mut self, dx: f64, dy: f64) {
    self.yaw -= dx * self.mouse_sensitivity;
    self.pitch -= dy * self.mouse_sensitivity;
    let pitch_limit = 89.0_f64.to_radians();
    let yaw_limit = 150.0_f64.to_radians();
    self.pitch = self.pitch.clamp(-pitch_limit, pitch_limit);
    self.yaw = self.yaw.clamp(-yaw_limit, yaw_limit);
}
```

**Key binding: press `C` to re-center head** (reset yaw/pitch to 0):
Add to the key handler in main.rs:
```rust
KeyCode::KeyC => {
    state.camera.yaw = 0.0;
    state.camera.pitch = 0.0;
}
```

---

## Problem 2: No Flight Data — Flying Blind

### Immediate fix: Console telemetry
Print flight state to stdout every 0.5 seconds. This is quick, no GPU text rendering needed.

**Add to `sim.rs` SimRunner:**

```rust
pub struct SimRunner {
    // ... existing fields ...
    telemetry_timer: f64,
}
```

Initialize `telemetry_timer: 0.0` in `SimRunner::new()`.

**In `SimRunner::update()`**, after the physics loop:

```rust
self.telemetry_timer += dt;
if self.telemetry_timer >= 0.5 {
    self.telemetry_timer = 0.0;
    self.print_telemetry();
}
```

**Add telemetry method:**

```rust
fn print_telemetry(&self) {
    let a = &self.sim.aircraft;
    let lat = a.lla.lat.to_degrees();
    let lon = a.lla.lon.to_degrees();
    let alt_ft = a.lla.alt * 3.28084;
    let gs_kts = a.groundspeed * 1.94384;
    let vs_fpm = a.vertical_speed * 196.85;
    let throttle_pct = self.sim.controls.throttle * 100.0;

    // Heading from body forward in ENU
    let nose_ecef = a.orientation * DVec3::X;
    let nose_enu = a.enu_frame.ecef_to_enu(nose_ecef);
    let hdg = nose_enu.x.atan2(nose_enu.y).to_degrees();
    let hdg = if hdg < 0.0 { hdg + 360.0 } else { hdg };

    // Pitch angle: body forward projected onto ENU up
    let pitch_deg = nose_enu.z.asin().to_degrees();

    // Bank angle: body right wing in ENU
    let right_ecef = a.orientation * DVec3::Y;
    let right_enu = a.enu_frame.ecef_to_enu(right_ecef);
    let bank_deg = right_enu.z.asin().to_degrees();

    println!(
        "HDG:{:5.1}° PIT:{:+5.1}° BNK:{:+5.1}° | \
         GS:{:5.1}kt VS:{:+6.0}fpm ALT:{:6.0}ft | \
         THR:{:3.0}% | \
         {:.4}°{} {:.4}°{}",
        hdg, pitch_deg, bank_deg,
        gs_kts, vs_fpm, alt_ft,
        throttle_pct,
        lat.abs(), if lat >= 0.0 { "N" } else { "S" },
        lon.abs(), if lon >= 0.0 { "E" } else { "W" },
    );
}
```

This gives you output like:
```
HDG:280.0° PIT:+0.0° BNK:+0.0° | GS:  0.0kt VS:   +0fpm ALT:     0ft | THR:  0% | 37.6139°N 122.3581°W
HDG:280.0° PIT:+0.0° BNK:+0.0° | GS: 15.3kt VS:   +0fpm ALT:     0ft | THR: 50% | 37.6139°N 122.3581°W
```

### Important: Use DVec3 import
The telemetry method uses `DVec3` — make sure `glam::DVec3` is imported in `sim.rs` (it already is).

---

## Summary of Changes

### Files to modify:
1. **`sim.rs`**:
   - Change `aircraft_view_matrix` signature to accept `head_yaw, head_pitch`
   - Add `telemetry_timer` field to SimRunner
   - Add `print_telemetry()` method
   - Call telemetry in `update()`

2. **`main.rs`**:
   - Pass `state.camera.yaw, state.camera.pitch` to `aircraft_view_matrix()`
   - Add KeyCode::KeyC handler to reset camera yaw/pitch

3. **`camera.rs`**:
   - Add yaw clamp to ±150° in `mouse_move()`
   - Remove all the WASD movement code from `update()` — camera no longer flies freely, it's locked to the aircraft. The `update()` method can be empty or removed entirely. Position is set by SimRunner.

### Files NOT to modify:
- `renderer.rs`
- `obj_loader.rs`
- `coords.rs`
- `physics.rs`
- `shaders/*`

### Test:
1. `cargo run --release`
2. Click to grab cursor
3. Move mouse — should look around cockpit
4. Press C — snaps view forward
5. Console should print telemetry every 0.5s
6. Hold Shift to increase throttle, watch GS increase in telemetry
7. Arrow keys should pitch/roll the aircraft