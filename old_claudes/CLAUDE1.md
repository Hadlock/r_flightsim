# CLAUDE.md — Wireframe Flight Sim Renderer

## Project Overview
A GPU-accelerated wireframe/edge-detection renderer for a flight simulator. Renders OBJ meshes and extracts their visible edges using screen-space Sobel edge detection, producing clean 1px white outlines on a dark blue background.

## Tech Stack
- **wgpu** — GPU rendering (cross-platform: Vulkan/Metal/DX12)
- **winit** — windowing (fullscreen + windowed, Win/Mac/Linux)
- **glam** — math (vec3/mat4, f64 for world coords via DVec3/DMat4)
- **tobj** — OBJ file loading
- No game engine, no ECS, no bevy.

## Visual Style
- Background color: `(0.10, 0.20, 0.30, 1.0)` — FSBLUE
  - As wgpu clear color: `wgpu::Color { r: 0.10, g: 0.20, b: 0.30, a: 1.0 }`
- Edge/line color: pure white `(1.0, 1.0, 1.0, 1.0)`
- All visible output is 1px white edges on FSBLUE background. No filled polygons visible in final output.

## Architecture

### Coordinate System
- World positions stored as `f64` (`glam::DVec3`) for future ECEF compatibility
- At render time, compute camera-relative positions in f64, then cast to f32 for GPU:
  ```rust
  let rel: DVec3 = object.world_pos - camera.world_pos;
  let model = Mat4::from_translation(rel.as_vec3());
  ```
- For now, use flat-earth X/Y/Z (X=east, Y=up, Z=north) but structure code so swapping to ECEF later only changes the world_pos values, not the rendering pipeline
- Camera near plane: 1.0m, far plane: 40000.0m (~25 miles visibility)

### Scene Objects
```rust
struct SceneObject {
    name: String,
    vertex_buf: wgpu::Buffer,    // vec3<f32> positions
    index_buf: wgpu::Buffer,     // u32 indices
    index_count: u32,
    world_pos: DVec3,            // f64 world position
    rotation: DQuat,             // f64 quaternion
    scale: f32,
    object_id: u32,              // unique ID for edge detection masking
    edges_enabled: bool,         // per-object toggle
}
```
- OBJ files loaded with `tobj`, vertices + indices uploaded to GPU buffers
- ~10 objects total: 3 airplanes (high poly), 1 airport, a few others
- One airplane moves at 15 m/s across ground (update world_pos each frame)

### Render Pipeline — Two Passes

**Pass 1: Geometry pass (offscreen MRT)**
Render all objects as filled triangles to three textures:
- **Depth texture** — `Depth32Float`
- **View-space normals** — `Rgba16Float`, encode normals from vertex shader
- **Object ID** — `R32Uint`, flat uint per object (from push constant or uniform)

Vertex shader: multiply position by MVP (camera-relative model * view * projection).
Fragment shader: output view-space normal + object ID. Depth written automatically.

**Pass 2: Sobel edge detection (fullscreen quad)**
- Sample depth, normal, and object-ID textures
- Run Sobel kernel (3x3) on depth and normals independently
- Detect edges where:
  - Depth discontinuity exceeds threshold (silhouette edges)
  - Normal discontinuity exceeds threshold (crease edges)
  - Object ID changes (object boundary edges)
- If `edges_enabled` is false for an object ID, discard that edge
- Output: white pixel where edge detected, FSBLUE where not
- Per-object edge thresholds can be passed via a storage buffer indexed by object ID

### Shaders (WGSL)
All shaders in `shaders/` directory as separate `.wgsl` files:
- `shaders/geometry.wgsl` — vertex + fragment for Pass 1
- `shaders/edge_detect.wgsl` — fullscreen quad vertex + Sobel fragment for Pass 2

### Camera
- Perspective projection, configurable FOV (default 60°)
- Position as DVec3, orientation as DQuat
- For now: simple fly camera with keyboard/mouse
- View matrix computed from camera pos/orientation each frame

## Project Structure
```
Cargo.toml
CLAUDE.md
src/
  main.rs          — winit event loop, wgpu init, frame loop
  renderer.rs      — pipeline creation, render pass execution
  scene.rs         — SceneObject, scene loading, object updates
  camera.rs        — camera state, input handling, view/proj matrices
  obj_loader.rs    — tobj wrapper, OBJ → vertex/index buffers
shaders/
  geometry.wgsl
  edge_detect.wgsl
assets/
  *.obj            — mesh files go here
```

## Key Implementation Notes
- Use `wgpu::PresentMode::Fifo` (vsync) by default
- Window should support both windowed and borderless fullscreen (toggle with F11)
- Resize handling: recreate depth/normal/ID textures + swapchain on resize
- No texture/material loading from OBJ files — ignore mtl, we only need geometry
- Index format: `u32` (not u16) since airplane meshes may exceed 65k verts
- For the fullscreen quad in Pass 2, use the vertex ID trick (no vertex buffer needed):
  ```wgsl
  let pos = vec2f(f32(vertex_index / 2u) * 4.0 - 1.0, f32(vertex_index % 2u) * 4.0 - 1.0);
  ```

## Build & Run
```bash
cargo run --release
```
Place OBJ files in `assets/`. The renderer loads all `*.obj` from that directory on startup.

## Do NOT
- Use bevy, macroquad, or any game engine
- Use f32 for world positions (must be f64 for ECEF readiness)
- Use localStorage or any web APIs (this is a native app)
- Add lighting, textures, or materials — this is a wireframe renderer
- Use LineList topology — we're using the Sobel approach for cleaner results
