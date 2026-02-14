mod ai_traffic;
mod aircraft_profile;
mod airport_gen;
mod airport_markers;
mod atc;
mod camera;
mod celestial;
mod cli;
mod coords;
mod earth;
mod menu;
mod obj_loader;
mod physics;
mod renderer;
mod scene;
mod sim;
mod telemetry;
mod tts;

use camera::Camera;
use clap::Parser;
use glam::Quat;
use renderer::Renderer;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Target frame time (~60 fps). The GPU vsync handles actual presentation timing;
/// this just prevents the CPU from busy-spinning between frames.
const TARGET_FRAME_TIME: Duration = Duration::from_micros(15_000);
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{DeviceEvent, DeviceId, ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Fullscreen, Window, WindowId},
};

const INITIAL_WIDTH: u32 = 800;
const INITIAL_HEIGHT: u32 = 600;

/// OBJ model -> body frame rotation.
/// OBJ has X=forward, Y=right, Z=up. Body has X=forward, Y=right, Z=down.
/// 180 deg around X flips Y and Z.
fn model_to_body_rotation() -> Quat {
    Quat::from_rotation_x(std::f32::consts::PI)
}

// ── GPU context (shared across states) ──────────────────────────────

struct GpuContext {
    window: Window,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    surface_format: wgpu::TextureFormat,
}

// ── Flying state ────────────────────────────────────────────────────

struct FlyingState {
    renderer: Renderer,
    camera: Camera,
    objects: Vec<scene::SceneObject>,
    last_frame: Instant,
    cursor_grabbed: bool,
    sim_runner: sim::SimRunner,
    aircraft_idx: usize,
    model_to_body: Quat,
    aircraft_name: String,
    earth_renderer: earth::EarthRenderer,
    earth_idx: usize,
    ai_traffic: ai_traffic::AiTrafficManager,
    atc_manager: atc::AtcManager,
    atc_states: Vec<atc::types::AiPlaneAtcState>,
    // egui for radio overlay
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    // TTS engine (None if --no-tts or init failed)
    tts_engine: Option<tts::TtsEngine>,
    // Celestial engine (sun, moon, planets, stars)
    celestial: celestial::CelestialEngine,
    celestial_indices: [usize; 5], // sun, moon, planets, prominent_stars, other_stars
    // Airport proximity markers
    airport_markers: Option<airport_markers::AirportMarkers>,
    marker_base_idx: usize,
}

// ── Game state enum ─────────────────────────────────────────────────

enum GameState {
    Menu(MenuStateWrapper),
    Flying(FlyingState),
}

struct MenuStateWrapper {
    renderer: Renderer,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    menu: menu::MenuState,
    last_frame: Instant,
}

// ── App ─────────────────────────────────────────────────────────────

struct App {
    gpu: Option<GpuContext>,
    game_state: Option<GameState>,
    args: cli::Args,
    shared_telemetry: telemetry::SharedTelemetry,
    dashboard_shutdown: Arc<AtomicBool>,
    dashboard_handle: Option<std::thread::JoinHandle<()>>,
}

impl App {
    fn new(args: cli::Args) -> Self {
        let shared_telemetry = telemetry::new_shared_telemetry();
        let dashboard_shutdown = Arc::new(AtomicBool::new(false));
        let dashboard_handle = telemetry::spawn_dashboard(
            shared_telemetry.clone(),
            dashboard_shutdown.clone(),
        );

        Self {
            gpu: None,
            game_state: None,
            args,
            shared_telemetry,
            dashboard_shutdown,
            dashboard_handle: Some(dashboard_handle),
        }
    }

    fn init_menu(&mut self) {
        let gpu = self.gpu.as_ref().expect("GPU not initialized");

        let profiles =
            aircraft_profile::load_all_profiles(Path::new("assets/planes"));

        let renderer = Renderer::new(
            &gpu.device,
            gpu.surface_format,
            gpu.config.width,
            gpu.config.height,
        );

        // egui setup
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui_ctx.viewport_id(),
            &gpu.window,
            None,
            None,
            None,
        );
        let egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.surface_format,
            None,
            1,
            false,
        );

        menu::configure_style(&egui_ctx);

        let mut menu = menu::MenuState::new(profiles);
        menu.request_preview_load();

        // Update telemetry to show menu state
        {
            let mut t = self.shared_telemetry.lock().unwrap();
            t.app_state = telemetry::AppStateLabel::Menu;
        }

        self.game_state = Some(GameState::Menu(MenuStateWrapper {
            renderer,
            egui_ctx,
            egui_state,
            egui_renderer,
            menu,
            last_frame: Instant::now(),
        }));
    }

    fn init_flying(&mut self, aircraft_slug: &str) {
        let gpu = self.gpu.as_ref().expect("GPU not initialized");

        let profiles =
            aircraft_profile::load_all_profiles(Path::new("assets/planes"));

        // Find the requested aircraft profile
        let profile = profiles
            .iter()
            .find(|p| p.slug == aircraft_slug)
            .or_else(|| profiles.first());

        let (params, aircraft_name, obj_path, wingspan) = match profile {
            Some(p) => (
                p.to_aircraft_params(),
                p.name.clone(),
                if p.has_model() {
                    Some(p.obj_path())
                } else {
                    None
                },
                p.physics.wing_span,
            ),
            None => {
                // Fallback to hardcoded Ki-61
                log::warn!("No aircraft profiles found, using hardcoded Ki-61");
                (
                    physics::AircraftParams::ki61(),
                    "Ki-61 Hien".to_string(),
                    None,
                    12.0,
                )
            }
        };

        let renderer = Renderer::new(
            &gpu.device,
            gpu.surface_format,
            gpu.config.width,
            gpu.config.height,
        );

        // Parse epoch early — used by both physics (L1) and celestial engine
        let epoch_unix = self.args.epoch.as_ref().and_then(|s| {
            match celestial::time::iso8601_to_unix(s) {
                Ok(unix) => Some(unix),
                Err(e) => {
                    log::warn!("Invalid --epoch '{}': {}, using system clock", s, e);
                    None
                }
            }
        });

        // Extract orbit spec before consuming profile data
        let orbit_spec = profile.as_ref().and_then(|p| p.orbit.clone());

        let mut camera = Camera::new(gpu.config.width as f32 / gpu.config.height as f32);
        // Set initial camera pitch for orbital profiles (e.g., -90° = looking down at Earth)
        if let Some(orbit) = &orbit_spec {
            camera.pitch = orbit.camera_pitch_deg.to_radians();
            if let Some(fov) = orbit.fov_deg {
                camera.fov_deg = fov as f32;
            }
        }

        // Physics setup — orbit or ground start
        let aircraft_body = match &orbit_spec {
            Some(orbit) if orbit.lagrange_point.is_some() => {
                let jd = celestial::time::unix_to_jd(
                    epoch_unix.unwrap_or_else(|| {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs_f64()
                    }),
                );
                physics::create_at_lagrange_point(orbit.altitude_km, jd)
            }
            Some(orbit) => physics::create_from_orbit(orbit),
            None => physics::create_aircraft_at_sfo(),
        };
        let simulation = physics::Simulation::new(params, aircraft_body);
        let sim_runner = sim::SimRunner::new(simulation);

        // Scene setup
        let mut objects = scene::load_scene(&gpu.device);

        // Airport generation
        let next_id = objects.iter().map(|o| o.object_id).max().unwrap_or(0) + 1;
        let airport_json = Path::new("assets/airports/airports_all.json");
        let ref_ecef = sim_runner.render_state().pos_ecef;
        let (airport_objects, next_id) =
            airport_gen::generate_airports(&gpu.device, airport_json, next_id, ref_ecef);
        objects.extend(airport_objects);

        // Earth mesh (WGS-84 ellipsoid)
        let (earth_renderer, earth_obj) = earth::EarthRenderer::new(&gpu.device);
        objects.push(earth_obj);
        let earth_idx = objects.len() - 1;

        // Aircraft object
        let aircraft_obj = match obj_path {
            Some(path) => {
                scene::load_aircraft_from_path(&gpu.device, &path, wingspan, next_id)
            }
            None => scene::load_aircraft_object(&gpu.device, next_id),
        };
        objects.push(aircraft_obj);
        let aircraft_idx = objects.len() - 1;

        // AI traffic planes
        let mut ai_traffic = ai_traffic::AiTrafficManager::new();
        let ki61_path = Path::new("assets/planes/ki61_hien/model.obj");
        let ai_base_id = next_id + 1;
        let mut ai_scene_indices = Vec::new();
        for i in 0..ai_traffic.plane_count() {
            let obj = scene::load_aircraft_from_path(
                &gpu.device,
                ki61_path,
                12.0,
                ai_base_id + i as u32,
            );
            ai_scene_indices.push(objects.len());
            objects.push(obj);
        }
        ai_traffic.set_scene_indices(ai_scene_indices);

        let model_to_body = model_to_body_rotation();

        // Celestial engine (sun, moon, planets, stars)
        let celestial_engine = celestial::CelestialEngine::new(epoch_unix);
        let next_celestial_id = ai_base_id + ai_traffic.plane_count() as u32;
        let (celestial_objects, celestial_rel_indices) =
            celestial_engine.create_scene_objects(&gpu.device, next_celestial_id);
        let celestial_base = objects.len();
        let celestial_indices = [
            celestial_base + celestial_rel_indices[0],
            celestial_base + celestial_rel_indices[1],
            celestial_base + celestial_rel_indices[2],
            celestial_base + celestial_rel_indices[3],
            celestial_base + celestial_rel_indices[4],
        ];
        objects.extend(celestial_objects);

        // Airport proximity markers — only in orbital mode (too large for airplane flight)
        let next_marker_id = next_celestial_id + 5;
        let mut airport_markers = if orbit_spec.is_some() {
            airport_markers::AirportMarkers::new(airport_json)
        } else {
            None
        };
        let marker_base_idx = objects.len();
        if let Some(markers) = &mut airport_markers {
            let marker_objects =
                markers.create_scene_objects(&gpu.device, next_marker_id, marker_base_idx);
            objects.extend(marker_objects);
        }

        // ATC system
        let num_ai = ai_traffic.plane_count();
        let mut atc_manager = atc::AtcManager::new(num_ai);
        let atc_states: Vec<atc::types::AiPlaneAtcState> = (0..num_ai)
            .map(|i| atc::build_atc_state(i))
            .collect();

        // TTS engine
        let tts_engine = if !self.args.no_tts {
            match tts::TtsEngine::new() {
                Ok(engine) => {
                    atc_manager.set_tts_sender(engine.tts_sender());
                    log::info!("TTS engine initialized");
                    Some(engine)
                }
                Err(e) => {
                    log::warn!("TTS disabled: {}", e);
                    None
                }
            }
        } else {
            log::info!("TTS disabled via --no-tts");
            None
        };

        // egui for flying radio overlay
        let egui_ctx = egui::Context::default();
        {
            let mut style = (*egui_ctx.style()).clone();
            style.visuals.window_fill = egui::Color32::TRANSPARENT;
            style.visuals.panel_fill = egui::Color32::TRANSPARENT;
            style.visuals.override_text_color = Some(egui::Color32::WHITE);
            egui_ctx.set_style(style);
        }
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui_ctx.viewport_id(),
            &gpu.window,
            None,
            None,
            None,
        );
        let egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.surface_format,
            None,
            1,
            false,
        );

        // Grab cursor
        let _ = gpu
            .window
            .set_cursor_grab(winit::window::CursorGrabMode::Confined)
            .or_else(|_| {
                gpu.window
                    .set_cursor_grab(winit::window::CursorGrabMode::Locked)
            });
        gpu.window.set_cursor_visible(false);

        log::info!(
            "Flying: {} ({} objects loaded)",
            aircraft_name,
            objects.len()
        );

        // Update telemetry
        {
            let mut t = self.shared_telemetry.lock().unwrap();
            t.app_state = telemetry::AppStateLabel::Flying;
            t.aircraft_name = aircraft_name.clone();
        }

        self.game_state = Some(GameState::Flying(FlyingState {
            renderer,
            camera,
            objects,
            last_frame: Instant::now(),
            cursor_grabbed: true,
            sim_runner,
            aircraft_idx,
            model_to_body,
            aircraft_name,
            earth_renderer,
            earth_idx,
            ai_traffic,
            atc_manager,
            atc_states,
            egui_ctx,
            egui_state,
            egui_renderer,
            tts_engine,
            celestial: celestial_engine,
            celestial_indices,
            airport_markers,
            marker_base_idx,
        }));
    }

    fn transition_to_menu(&mut self, previous_selection: Option<usize>) {
        let gpu = self.gpu.as_ref().expect("GPU not initialized");

        // Release cursor
        let _ = gpu
            .window
            .set_cursor_grab(winit::window::CursorGrabMode::None);
        gpu.window.set_cursor_visible(true);

        let profiles =
            aircraft_profile::load_all_profiles(Path::new("assets/planes"));

        let renderer = Renderer::new(
            &gpu.device,
            gpu.surface_format,
            gpu.config.width,
            gpu.config.height,
        );

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui_ctx.viewport_id(),
            &gpu.window,
            None,
            None,
            None,
        );
        let egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.surface_format,
            None,
            1,
            false,
        );

        menu::configure_style(&egui_ctx);

        let mut menu = menu::MenuState::new(profiles);
        if let Some(idx) = previous_selection {
            menu.selected_index = idx;
        }
        menu.request_preview_load();

        {
            let mut t = self.shared_telemetry.lock().unwrap();
            t.app_state = telemetry::AppStateLabel::Menu;
        }

        self.game_state = Some(GameState::Menu(MenuStateWrapper {
            renderer,
            egui_ctx,
            egui_state,
            egui_renderer,
            menu,
            last_frame: Instant::now(),
        }));
    }

    fn handle_flying_event(
        flying: &mut FlyingState,
        gpu: &GpuContext,
        event: &WindowEvent,
    ) -> FlyingAction {
        match event {
            WindowEvent::Resized(new_size) => {
                let w = new_size.width.max(1);
                let h = new_size.height.max(1);
                flying.renderer.resize(&gpu.device, w, h);
                flying.camera.aspect = w as f32 / h as f32;
                FlyingAction::None
            }

            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key),
                        state: key_state,
                        ..
                    },
                ..
            } => match key_state {
                ElementState::Pressed => match key {
                    KeyCode::Escape => FlyingAction::ReturnToMenu,
                    KeyCode::F11 => {
                        if gpu.window.fullscreen().is_some() {
                            gpu.window.set_fullscreen(None);
                        } else {
                            gpu.window
                                .set_fullscreen(Some(Fullscreen::Borderless(None)));
                        }
                        FlyingAction::None
                    }
                    KeyCode::KeyC => {
                        flying.camera.yaw = 0.0;
                        flying.camera.pitch = 0.0;
                        FlyingAction::None
                    }
                    KeyCode::KeyP => {
                        flying.celestial.star_toggle =
                            flying.celestial.star_toggle.cycle();
                        FlyingAction::None
                    }
                    _ => {
                        flying.sim_runner.key_down(*key);
                        FlyingAction::None
                    }
                },
                ElementState::Released => {
                    flying.sim_runner.key_up(*key);
                    FlyingAction::None
                }
            },

            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                if !flying.cursor_grabbed {
                    flying.cursor_grabbed = true;
                    let _ = gpu
                        .window
                        .set_cursor_grab(winit::window::CursorGrabMode::Confined)
                        .or_else(|_| {
                            gpu.window
                                .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                        });
                    gpu.window.set_cursor_visible(false);
                }
                FlyingAction::None
            }

            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now.duration_since(flying.last_frame).as_secs_f64();
                flying.last_frame = now;

                // Advance physics
                flying.sim_runner.update(dt);

                // Get interpolated render state
                let render_state = flying.sim_runner.render_state();

                // Update camera position to pilot eye
                flying.camera.position =
                    flying.sim_runner.camera_position(&render_state);

                // Update clip planes and earth mesh for current altitude
                let altitude_m = flying.sim_runner.sim.aircraft.lla.alt;
                flying.camera.update_clip_planes(altitude_m);

                // View matrix from aircraft orientation + pilot head look
                let view = sim::aircraft_view_matrix(
                    render_state.orientation,
                    flying.camera.yaw,
                    flying.camera.pitch,
                );
                let proj = flying.camera.projection_matrix();

                flying.earth_renderer.update(
                    &gpu.device,
                    &gpu.queue,
                    &mut flying.objects[flying.earth_idx],
                    flying.camera.position,
                    altitude_m,
                );

                // Update aircraft SceneObject from physics
                let aircraft = &mut flying.objects[flying.aircraft_idx];
                aircraft.world_pos = render_state.pos_ecef;
                aircraft.rotation =
                    sim::dquat_to_quat(render_state.orientation) * flying.model_to_body;

                // Update celestial bodies (sun, moon, planets, stars)
                flying.celestial.update(dt, flying.camera.position);
                flying.celestial.update_scene_objects(
                    &gpu.device,
                    &gpu.queue,
                    &mut flying.objects,
                    &flying.celestial_indices,
                    flying.camera.position,
                    altitude_m,
                    flying.camera.far,
                );

                // Update AI traffic
                flying.ai_traffic.update(dt);
                let ai_count = flying.ai_traffic.plane_count();
                for i in 0..ai_count {
                    let idx = flying.ai_traffic.scene_indices()[i];
                    let pos = flying.ai_traffic.planes()[i].pos_ecef;
                    let orient = flying.ai_traffic.planes()[i].orientation;
                    flying.objects[idx].world_pos = pos;
                    flying.objects[idx].rotation =
                        sim::dquat_to_quat(orient) * flying.model_to_body;
                }

                // Update airport proximity markers
                if let Some(markers) = &mut flying.airport_markers {
                    markers.update(dt, flying.camera.position, &mut flying.objects);
                }

                // ATC tick
                flying.atc_manager.advance_enroute_timers(dt);
                flying.atc_manager.tick(
                    dt,
                    flying.ai_traffic.planes(),
                    &mut flying.atc_states,
                    render_state.pos_ecef,
                );

                // Cull invisible objects: save index_count, zero it, render, restore.
                // Uses bounding sphere so large objects (runways, building clusters)
                // don't pop out when their center passes behind the camera.
                const DISTANCE_CULL_M: f64 = 400_000.0; // ~250 miles
                let culled: Vec<(usize, u32)> = flying
                    .objects
                    .iter()
                    .enumerate()
                    .filter_map(|(i, obj)| {
                        // Never cull earth, player aircraft, celestial bodies,
                        // or airport markers (markers self-manage visibility)
                        if i == flying.earth_idx
                            || i == flying.aircraft_idx
                            || flying.celestial_indices.contains(&i)
                            || (i >= flying.marker_base_idx
                                && i < flying.marker_base_idx + 1024)
                        {
                            return None;
                        }
                        let rel = obj.world_pos - flying.camera.position;
                        let dist = rel.length();
                        let radius = obj.bounding_radius as f64;
                        // Distance cull: nearest point of bounding sphere beyond limit
                        if dist - radius > DISTANCE_CULL_M {
                            return Some((i, obj.index_count));
                        }
                        // Behind-camera cull: entire bounding sphere behind near plane
                        let rel_f32 = rel.as_vec3();
                        let view_z = view.transform_point3(rel_f32).z;
                        if view_z > obj.bounding_radius {
                            return Some((i, obj.index_count));
                        }
                        None
                    })
                    .collect();
                for &(i, _) in &culled {
                    flying.objects[i].index_count = 0;
                }

                // Render
                let output = match gpu.surface.get_current_texture() {
                    Ok(t) => t,
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        // Restore culled counts before returning
                        for (i, count) in culled {
                            flying.objects[i].index_count = count;
                        }
                        return FlyingAction::ReconfigureSurface;
                    }
                    Err(e) => {
                        log::error!("Surface error: {}", e);
                        for (i, count) in culled {
                            flying.objects[i].index_count = count;
                        }
                        return FlyingAction::None;
                    }
                };

                let surface_view = output
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                // Solid overlay: sun + airport markers (bypasses Sobel edge detection,
                // avoiding depth buffer precision artifacts at orbital distance).
                let sun_idx = flying.celestial_indices[0];
                let mut overlay_indices = vec![sun_idx];
                for i in 0..1024 {
                    let idx = flying.marker_base_idx + i;
                    if idx < flying.objects.len()
                        && flying.objects[idx].index_count > 0
                    {
                        overlay_indices.push(idx);
                    }
                }
                flying.renderer.render(
                    &gpu.device,
                    &gpu.queue,
                    &surface_view,
                    &flying.objects,
                    view,
                    proj,
                    flying.camera.position,
                    &overlay_indices,
                );

                // Restore culled objects' index counts
                for (i, count) in culled {
                    flying.objects[i].index_count = count;
                }

                // egui radio overlay
                let raw_input = flying.egui_state.take_egui_input(&gpu.window);
                let recent = flying.atc_manager.recent_messages(15.0);
                let com1 = flying.atc_manager.com1_freq;

                let full_output = flying.egui_ctx.run(raw_input, |ctx| {
                    draw_radio_overlay(ctx, &recent, com1);
                });

                flying.egui_state.handle_platform_output(
                    &gpu.window,
                    full_output.platform_output,
                );

                let clipped = flying.egui_ctx.tessellate(
                    full_output.shapes,
                    full_output.pixels_per_point,
                );

                let screen_desc = egui_wgpu::ScreenDescriptor {
                    size_in_pixels: [gpu.config.width, gpu.config.height],
                    pixels_per_point: full_output.pixels_per_point,
                };

                for (id, delta) in &full_output.textures_delta.set {
                    flying.egui_renderer.update_texture(
                        &gpu.device, &gpu.queue, *id, delta,
                    );
                }

                let mut encoder = gpu.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor {
                        label: Some("egui flying encoder"),
                    },
                );

                flying.egui_renderer.update_buffers(
                    &gpu.device,
                    &gpu.queue,
                    &mut encoder,
                    &clipped,
                    &screen_desc,
                );

                {
                    let mut pass = encoder.begin_render_pass(
                        &wgpu::RenderPassDescriptor {
                            label: Some("egui flying pass"),
                            color_attachments: &[Some(
                                wgpu::RenderPassColorAttachment {
                                    view: &surface_view,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                },
                            )],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        },
                    ).forget_lifetime();

                    flying.egui_renderer.render(
                        &mut pass, &clipped, &screen_desc,
                    );
                }

                gpu.queue.submit(std::iter::once(encoder.finish()));

                for id in &full_output.textures_delta.free {
                    flying.egui_renderer.free_texture(id);
                }

                output.present();

                // Frame pacing: yield CPU until next target frame
                if let Some(remaining) = TARGET_FRAME_TIME.checked_sub(now.elapsed()) {
                    std::thread::sleep(remaining);
                }

                gpu.window.request_redraw();

                FlyingAction::UpdateTelemetry
            }

            _ => FlyingAction::None,
        }
    }
}

enum FlyingAction {
    None,
    ReturnToMenu,
    ReconfigureSurface,
    UpdateTelemetry,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }

        let fullscreen = if self.args.windowed {
            None
        } else {
            Some(Fullscreen::Borderless(None))
        };

        let window_attrs = Window::default_attributes()
            .with_title("shaderflight")
            .with_inner_size(PhysicalSize::new(INITIAL_WIDTH, INITIAL_HEIGHT))
            .with_fullscreen(fullscreen);

        let window = event_loop
            .create_window(window_attrs)
            .expect("Failed to create window");

        // wgpu setup
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        // SAFETY: window lives as long as surface (both in GpuContext)
        let surface = unsafe {
            std::mem::transmute::<wgpu::Surface<'_>, wgpu::Surface<'static>>(
                instance
                    .create_surface(&window)
                    .expect("Failed to create surface"),
            )
        };

        let adapter = pollster::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            },
        ))
        .expect("Failed to find GPU adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("GPU Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            },
            None,
        ))
        .expect("Failed to create device");

        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| !f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        log::info!("GPU initialized, surface format: {:?}", surface_format);

        self.gpu = Some(GpuContext {
            window,
            surface,
            device,
            queue,
            config,
            surface_format,
        });

        // Initialize game state based on CLI args
        if self.args.instant {
            let aircraft = self.args.aircraft.clone();
            self.init_flying(&aircraft);
        } else {
            self.init_menu();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.gpu.is_none() {
            return;
        }

        // Handle close
        if matches!(event, WindowEvent::CloseRequested) {
            event_loop.exit();
            return;
        }

        // Handle resize for GPU config
        if let WindowEvent::Resized(new_size) = event {
            let w = new_size.width.max(1);
            let h = new_size.height.max(1);
            // We need mutable access to gpu for config update
            if let Some(gpu) = self.gpu.as_mut() {
                gpu.config.width = w;
                gpu.config.height = h;
                gpu.surface.configure(&gpu.device, &gpu.config);
            }
        }

        let game_state = match self.game_state.take() {
            Some(s) => s,
            None => return,
        };

        match game_state {
            GameState::Menu(mut menu_state) => {
                // Let egui process the event
                let gpu = self.gpu.as_ref().unwrap();
                let response =
                    menu_state
                        .egui_state
                        .on_window_event(&gpu.window, &event);

                if !response.consumed {
                    // Handle non-egui events
                    match &event {
                        WindowEvent::Resized(new_size) => {
                            let w = new_size.width.max(1);
                            let h = new_size.height.max(1);
                            menu_state.renderer.resize(&gpu.device, w, h);
                        }
                        WindowEvent::KeyboardInput {
                            event:
                                KeyEvent {
                                    physical_key: PhysicalKey::Code(KeyCode::F11),
                                    state: ElementState::Pressed,
                                    ..
                                },
                            ..
                        } => {
                            if gpu.window.fullscreen().is_some() {
                                gpu.window.set_fullscreen(None);
                            } else {
                                gpu.window
                                    .set_fullscreen(Some(Fullscreen::Borderless(None)));
                            }
                        }
                        WindowEvent::KeyboardInput {
                            event:
                                KeyEvent {
                                    physical_key: PhysicalKey::Code(KeyCode::Space),
                                    state: ElementState::Pressed,
                                    repeat: false,
                                    ..
                                },
                            ..
                        } => {
                            menu_state.menu.preview_paused =
                                !menu_state.menu.preview_paused;
                        }
                        _ => {}
                    }
                }

                if let WindowEvent::RedrawRequested = event {
                    let now = Instant::now();
                    let dt = now.duration_since(menu_state.last_frame).as_secs_f32();
                    menu_state.last_frame = now;

                    // Poll arrow keys for smooth preview control
                    {
                        let mut yaw = 0.0_f32;
                        let mut pitch = 0.0_f32;
                        // Arrow key state is not tracked by egui; query the egui
                        // input events that arrived this frame.
                        let ctx = &menu_state.egui_ctx;
                        ctx.input(|input| {
                            if input.key_down(egui::Key::ArrowRight) {
                                yaw += 1.0;
                            }
                            if input.key_down(egui::Key::ArrowLeft) {
                                yaw -= 1.0;
                            }
                            if input.key_down(egui::Key::ArrowUp) {
                                pitch += 1.0;
                            }
                            if input.key_down(egui::Key::ArrowDown) {
                                pitch -= 1.0;
                            }
                        });
                        if yaw != 0.0 || pitch != 0.0 {
                            menu_state.menu.apply_arrow_input(yaw, pitch, dt);
                        }
                    }

                    // Update preview rotation/pitch/zoom
                    menu_state.menu.update_preview(dt);

                    // Check for pending model loads
                    menu_state
                        .menu
                        .poll_preview_load(&gpu.device);

                    // Run egui
                    let raw_input = menu_state.egui_state.take_egui_input(&gpu.window);
                    let full_output = menu_state.egui_ctx.run(raw_input, |ctx| {
                        menu_state.menu.draw_ui(ctx);
                    });

                    menu_state.egui_state.handle_platform_output(
                        &gpu.window,
                        full_output.platform_output,
                    );

                    // Render 3D preview
                    let output = match gpu.surface.get_current_texture() {
                        Ok(t) => t,
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            if let Some(gpu) = self.gpu.as_ref() {
                                gpu.surface.configure(&gpu.device, &gpu.config);
                            }
                            self.game_state = Some(GameState::Menu(menu_state));
                            return;
                        }
                        Err(e) => {
                            log::error!("Surface error: {}", e);
                            self.game_state = Some(GameState::Menu(menu_state));
                            return;
                        }
                    };

                    let surface_view = output
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());

                    // Preview camera: 3/4 overhead view, 45deg FOV
                    let preview_camera = Camera {
                        position: glam::DVec3::ZERO,
                        yaw: 0.0,
                        pitch: 0.0,
                        fov_deg: 45.0,
                        aspect: gpu.config.width as f32 / gpu.config.height as f32,
                        near: 0.1,
                        far: 1000.0,
                        mouse_sensitivity: 0.0,
                    };

                    // Camera looking down at 30deg elevation from front-quarter
                    let elev = 30.0_f32.to_radians();
                    let azimuth = std::f32::consts::FRAC_PI_4; // 45 deg offset
                    let dist = 25.0_f32 / menu_state.menu.preview_zoom;
                    let cam_x = dist * elev.cos() * azimuth.cos();
                    let cam_y = dist * elev.cos() * azimuth.sin();
                    let cam_z = dist * elev.sin();
                    let eye = glam::Vec3::new(cam_x, cam_y, cam_z);
                    let center = glam::Vec3::ZERO;
                    let up = glam::Vec3::Z;
                    let view = glam::Mat4::look_at_rh(eye, center, up);
                    let proj = preview_camera.projection_matrix();

                    // Update preview rotation (yaw around Z, pitch around local Y)
                    let yaw = Quat::from_rotation_z(menu_state.menu.preview_rotation);
                    let pitch =
                        Quat::from_rotation_y(menu_state.menu.preview_pitch);
                    let rotation = yaw * pitch;
                    if let Some(ref mut obj) = menu_state.menu.preview_object {
                        obj.rotation = rotation;
                    }

                    // Render Sobel pass (preview)
                    if menu_state.menu.preview_object.is_some() {
                        menu_state.renderer.render(
                            &gpu.device,
                            &gpu.queue,
                            &surface_view,
                            std::slice::from_ref(
                                menu_state.menu.preview_object.as_ref().unwrap(),
                            ),
                            view,
                            proj,
                            glam::DVec3::ZERO,
                            &[],
                        );
                    } else {
                        // Render empty scene (just FSBLUE background)
                        menu_state.renderer.render(
                            &gpu.device,
                            &gpu.queue,
                            &surface_view,
                            &[],
                            view,
                            proj,
                            glam::DVec3::ZERO,
                            &[],
                        );
                    }

                    // Render egui overlay on top
                    let clipped_primitives = menu_state.egui_ctx.tessellate(
                        full_output.shapes,
                        full_output.pixels_per_point,
                    );

                    let screen_descriptor = egui_wgpu::ScreenDescriptor {
                        size_in_pixels: [gpu.config.width, gpu.config.height],
                        pixels_per_point: full_output.pixels_per_point,
                    };

                    for (id, delta) in &full_output.textures_delta.set {
                        menu_state
                            .egui_renderer
                            .update_texture(&gpu.device, &gpu.queue, *id, delta);
                    }

                    let mut encoder = gpu.device.create_command_encoder(
                        &wgpu::CommandEncoderDescriptor {
                            label: Some("egui Encoder"),
                        },
                    );

                    menu_state.egui_renderer.update_buffers(
                        &gpu.device,
                        &gpu.queue,
                        &mut encoder,
                        &clipped_primitives,
                        &screen_descriptor,
                    );

                    {
                        let mut pass =
                            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("egui Pass"),
                                color_attachments: &[Some(
                                    wgpu::RenderPassColorAttachment {
                                        view: &surface_view,
                                        resolve_target: None,
                                        ops: wgpu::Operations {
                                            load: wgpu::LoadOp::Load,
                                            store: wgpu::StoreOp::Store,
                                        },
                                    },
                                )],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                            })
                            .forget_lifetime();

                        menu_state.egui_renderer.render(
                            &mut pass,
                            &clipped_primitives,
                            &screen_descriptor,
                        );
                    }

                    gpu.queue.submit(std::iter::once(encoder.finish()));
                    output.present();

                    for id in &full_output.textures_delta.free {
                        menu_state.egui_renderer.free_texture(id);
                    }

                    // Frame pacing: yield CPU until next target frame
                    if let Some(remaining) = TARGET_FRAME_TIME.checked_sub(now.elapsed()) {
                        std::thread::sleep(remaining);
                    }

                    gpu.window.request_redraw();

                    // Check if Fly Now was clicked
                    if menu_state.menu.fly_now_clicked {
                        let slug = menu_state
                            .menu
                            .selected_profile()
                            .map(|p| p.slug.clone())
                            .unwrap_or_else(|| "ki61_hien".to_string());
                        // Drop menu state before transitioning
                        drop(menu_state);
                        self.init_flying(&slug);
                        return;
                    }
                }

                self.game_state = Some(GameState::Menu(menu_state));
            }

            GameState::Flying(mut flying) => {
                let gpu = self.gpu.as_ref().unwrap();
                let action = Self::handle_flying_event(&mut flying, gpu, &event);

                match action {
                    FlyingAction::ReturnToMenu => {
                        // Find the index of the current aircraft in profiles
                        let profiles = aircraft_profile::load_all_profiles(
                            Path::new("assets/planes"),
                        );
                        let prev_idx = profiles
                            .iter()
                            .position(|p| p.name == flying.aircraft_name);
                        drop(flying);
                        self.transition_to_menu(prev_idx);
                        return;
                    }
                    FlyingAction::ReconfigureSurface => {
                        if let Some(gpu) = self.gpu.as_ref() {
                            gpu.surface.configure(&gpu.device, &gpu.config);
                        }
                    }
                    FlyingAction::UpdateTelemetry => {
                        // Publish telemetry
                        self.publish_flying_telemetry(&flying);
                    }
                    FlyingAction::None => {}
                }

                self.game_state = Some(GameState::Flying(flying));
            }
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if let Some(GameState::Flying(ref mut flying)) = self.game_state {
                if flying.cursor_grabbed {
                    flying.camera.mouse_move(dx, dy);
                }
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Rendering is driven by request_redraw() in RedrawRequested handlers.
        // Don't request_redraw() here — it creates a busy loop that pins the CPU
        // at 100% because macOS Metal present() doesn't block the CPU thread.
    }
}

/// Draw the radio overlay in the top-right corner during flying state.
fn draw_radio_overlay(
    ctx: &egui::Context,
    messages: &[&atc::types::RadioMessage],
    com1_freq: f32,
) {
    egui::Area::new(egui::Id::new("radio_overlay"))
        .anchor(egui::Align2::RIGHT_TOP, egui::Vec2::new(-10.0, 10.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(egui::Color32::from_rgba_unmultiplied(25, 51, 76, 200))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(380.0);

                    // Frequency header
                    ui.label(
                        egui::RichText::new(format!("COM1: {:.1}", com1_freq))
                            .color(egui::Color32::from_rgb(120, 180, 220))
                            .small()
                            .strong(),
                    );

                    ui.add_space(4.0);

                    // Show last 4 messages
                    let display_msgs: Vec<_> = messages.iter().rev().take(4).rev().collect();

                    if display_msgs.is_empty() {
                        ui.label(
                            egui::RichText::new("  monitoring...")
                                .color(egui::Color32::from_rgb(100, 120, 140))
                                .small(),
                        );
                    } else {
                        for msg in display_msgs {
                            let is_controller = matches!(
                                msg.speaker,
                                atc::types::Speaker::Controller(_)
                            );
                            let speaker_color = if is_controller {
                                egui::Color32::from_rgb(140, 220, 255) // light cyan
                            } else {
                                egui::Color32::from_rgb(180, 190, 200) // light gray
                            };
                            let text_color = if is_controller {
                                egui::Color32::from_rgb(220, 235, 245)
                            } else {
                                egui::Color32::from_rgb(170, 180, 190)
                            };

                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{}:", msg.display_speaker))
                                        .color(speaker_color)
                                        .small()
                                        .strong(),
                                );
                                ui.label(
                                    egui::RichText::new(&msg.text)
                                        .color(text_color)
                                        .small(),
                                );
                            });
                        }
                    }
                });
        });
}

impl App {
    fn publish_flying_telemetry(&self, flying: &FlyingState) {
        let sim = &flying.sim_runner.sim;
        let a = &sim.aircraft;
        let c = &sim.controls;

        let lat = a.lla.lat.to_degrees();
        let lon = a.lla.lon.to_degrees();
        let alt_ft = a.lla.alt * 3.28084;
        let agl_ft = a.agl * 3.28084;
        let gs_kts = a.groundspeed * 1.94384;
        let vs_fpm = a.vertical_speed * 196.85;

        // Heading
        let nose_ecef = a.orientation * glam::DVec3::X;
        let nose_enu = a.enu_frame.ecef_to_enu(nose_ecef);
        let hdg = nose_enu.x.atan2(nose_enu.y).to_degrees();
        let hdg = if hdg < 0.0 { hdg + 360.0 } else { hdg };

        // Pitch
        let pitch_deg = nose_enu.z.asin().to_degrees();

        // Bank
        let right_ecef = a.orientation * glam::DVec3::Y;
        let right_enu = a.enu_frame.ecef_to_enu(right_ecef);
        let bank_deg = right_enu.z.asin().to_degrees();

        // Airspeed (body frame forward velocity)
        let vel_body = a.orientation.conjugate() * a.vel_ecef;
        let airspeed_kts = vel_body.length() * 1.94384;

        // Alpha
        let alpha_deg = if vel_body.x.abs() > 0.1 {
            vel_body.z.atan2(vel_body.x).to_degrees()
        } else {
            0.0
        };

        // Build radio log from ATC message log
        let radio_log: Vec<telemetry::RadioLogEntry> = flying
            .atc_manager
            .message_log()
            .iter()
            .rev()
            .take(20)
            .rev()
            .map(|m| telemetry::RadioLogEntry {
                frequency: m.frequency,
                speaker: m.display_speaker.clone(),
                text: m.text.clone(),
            })
            .collect();

        let mut t = self.shared_telemetry.lock().unwrap();
        t.airspeed_kts = airspeed_kts;
        t.groundspeed_kts = gs_kts;
        t.vertical_speed_fpm = vs_fpm;
        t.altitude_msl_ft = alt_ft;
        t.altitude_agl_ft = agl_ft;
        t.heading_deg = hdg;
        t.pitch_deg = pitch_deg;
        t.bank_deg = bank_deg;
        t.throttle_pct = c.throttle * 100.0;
        t.alpha_deg = alpha_deg;
        t.on_ground = a.on_ground;
        t.brakes = c.brakes > 0.0;
        t.latitude = lat;
        t.longitude = lon;
        t.radio_log = radio_log;
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // Signal dashboard thread to shut down
        self.dashboard_shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.dashboard_handle.take() {
            let _ = handle.join();
        }
    }
}

fn main() {
    env_logger::init();

    let args = cli::Args::parse();

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut app = App::new(args);
    event_loop.run_app(&mut app).expect("Event loop error");
}
