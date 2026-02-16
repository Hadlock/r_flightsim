mod ai_traffic;
mod aircraft_profile;
mod airport_gen;
mod airport_markers;
mod atc;
mod audio;
mod camera;
mod celestial;
mod cli;
mod constants;
mod coords;
mod earth;
mod flying;
mod menu;
mod settings;
mod obj_loader;
mod physics;
mod renderer;
mod scene;
mod sim;
mod telemetry;
mod tle;
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

// ── Egui context (shared init + render boilerplate) ────────────────

struct EguiContext {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}

impl EguiContext {
    fn new(gpu: &GpuContext) -> Self {
        let ctx = egui::Context::default();
        let state = egui_winit::State::new(
            ctx.clone(),
            ctx.viewport_id(),
            &gpu.window,
            None,
            None,
            None,
        );
        let renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.surface_format,
            None,
            1,
            false,
        );
        Self { ctx, state, renderer }
    }

    /// Run egui, tessellate, render to the given surface view.
    fn render_to_surface(
        &mut self,
        gpu: &GpuContext,
        surface_view: &wgpu::TextureView,
        ui_fn: impl FnMut(&egui::Context),
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
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
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

use flying::{FlyingState, FlyingAction};

// ── Game state enum ─────────────────────────────────────────────────

enum GameState {
    Menu(MenuStateWrapper),
    Flying(FlyingState),
}

struct MenuStateWrapper {
    renderer: Renderer,
    egui: EguiContext,
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
    /// Pre-parsed airport data (loaded once at menu init, reused across flights)
    parsed_airports: Option<airport_gen::ParsedAirports>,
    /// Aircraft profiles (loaded once, reused across menu/flying transitions)
    cached_profiles: Option<Vec<aircraft_profile::AircraftProfile>>,
    settings: settings::Settings,
    music_player: Option<audio::MusicPlayer>,
}

impl App {
    fn new(args: cli::Args) -> Self {
        let shared_telemetry = telemetry::new_shared_telemetry();
        let dashboard_shutdown = Arc::new(AtomicBool::new(false));
        let dashboard_handle = telemetry::spawn_dashboard(
            shared_telemetry.clone(),
            dashboard_shutdown.clone(),
        );

        let settings = settings::Settings::new();
        let music_player = audio::MusicPlayer::new(
            Path::new("assets/music"),
            settings.music_volume.clone(),
        );

        Self {
            gpu: None,
            game_state: None,
            args,
            shared_telemetry,
            dashboard_shutdown,
            dashboard_handle: Some(dashboard_handle),
            parsed_airports: None,
            cached_profiles: None,
            settings,
            music_player,
        }
    }

    fn ensure_profiles(&mut self) -> &[aircraft_profile::AircraftProfile] {
        if self.cached_profiles.is_none() {
            self.cached_profiles = Some(
                aircraft_profile::load_all_profiles(Path::new("assets/planes"))
            );
        }
        self.cached_profiles.as_ref().unwrap()
    }

    fn init_menu(&mut self) {
        // Ensure music is playing
        if self.music_player.is_none() {
            self.music_player = audio::MusicPlayer::new(
                Path::new("assets/music"),
                self.settings.music_volume.clone(),
            );
        }

        let profiles = self.ensure_profiles().to_vec();
        let gpu = self.gpu.as_ref().expect("GPU not initialized");

        let renderer = Renderer::new(
            &gpu.device,
            gpu.surface_format,
            gpu.config.width,
            gpu.config.height,
        );

        let egui = EguiContext::new(gpu);
        menu::configure_style(&egui.ctx);

        let music_pct = (self.settings.music_volume.get() * 100.0).round() as u32;
        let atc_pct = (self.settings.atc_voice_volume.get() * 100.0).round() as u32;
        let engine_pct = (self.settings.engine_volume.get() * 100.0).round() as u32;
        let mut menu = menu::MenuState::new(
            profiles,
            music_pct,
            atc_pct,
            engine_pct,
            self.settings.fetch_orbital_params,
        );
        menu.request_preview_load();

        // Update telemetry to show menu state
        {
            let mut t = self.shared_telemetry.lock().unwrap();
            t.app_state = telemetry::AppStateLabel::Menu;
        }

        self.game_state = Some(GameState::Menu(MenuStateWrapper {
            renderer,
            egui,
            menu,
            last_frame: Instant::now(),
        }));

        // Pre-parse airport JSON while user browses the menu
        if self.parsed_airports.is_none() {
            let t = Instant::now();
            let json = std::fs::read_to_string("assets/airports/airports_all.json")
                .unwrap_or_default();
            self.parsed_airports = Some(airport_gen::parse_airports_json(&json));
            log::info!("[init_menu] pre-parsed airports: {:.0}ms", t.elapsed().as_millis());
        }
    }

    fn init_flying(&mut self, aircraft_slug: &str) {
        let profiles = self.ensure_profiles().to_vec();
        let gpu = self.gpu.as_ref().expect("GPU not initialized");

        let profile = profiles
            .iter()
            .find(|p| p.slug == aircraft_slug)
            .or_else(|| profiles.first());

        let epoch_unix = self.args.epoch.as_ref().and_then(|s| {
            match celestial::time::iso8601_to_unix(s) {
                Ok(unix) => Some(unix),
                Err(e) => {
                    log::warn!("Invalid --epoch '{}': {}, using system clock", s, e);
                    None
                }
            }
        });

        let atc_volume = if self.args.no_tts {
            None
        } else {
            Some(self.settings.atc_voice_volume.clone())
        };

        let flying = FlyingState::new(
            gpu,
            profile,
            &mut self.parsed_airports,
            epoch_unix,
            self.args.no_tts,
            atc_volume,
            self.settings.engine_volume.clone(),
            self.settings.fetch_orbital_params,
        );

        // Update telemetry
        {
            let mut t = self.shared_telemetry.lock().unwrap();
            t.app_state = telemetry::AppStateLabel::Flying;
            t.aircraft_name = flying.aircraft_name.clone();
        }

        self.game_state = Some(GameState::Flying(flying));
    }

    fn transition_to_menu(&mut self, previous_selection: Option<usize>) {
        // Release cursor
        {
            let gpu = self.gpu.as_ref().expect("GPU not initialized");
            let _ = gpu
                .window
                .set_cursor_grab(winit::window::CursorGrabMode::None);
            gpu.window.set_cursor_visible(true);
        }

        let profiles = self.ensure_profiles().to_vec();
        let gpu = self.gpu.as_ref().expect("GPU not initialized");

        let renderer = Renderer::new(
            &gpu.device,
            gpu.surface_format,
            gpu.config.width,
            gpu.config.height,
        );

        let egui = EguiContext::new(gpu);
        menu::configure_style(&egui.ctx);

        let music_pct = (self.settings.music_volume.get() * 100.0).round() as u32;
        let atc_pct = (self.settings.atc_voice_volume.get() * 100.0).round() as u32;
        let engine_pct = (self.settings.engine_volume.get() * 100.0).round() as u32;
        let mut menu = menu::MenuState::new(
            profiles,
            music_pct,
            atc_pct,
            engine_pct,
            self.settings.fetch_orbital_params,
        );
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
            egui,
            menu,
            last_frame: Instant::now(),
        }));
    }

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
                        .egui
                        .state
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
                        let ctx = &menu_state.egui.ctx;
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

                    // Advance music and sync settings sliders to SharedVolume
                    if let Some(ref mut player) = self.music_player {
                        player.tick();
                    }
                    self.settings.music_volume.set(menu_state.menu.settings_music_pct as f32 / 100.0);
                    self.settings.atc_voice_volume.set(menu_state.menu.settings_atc_pct as f32 / 100.0);
                    self.settings.engine_volume.set(menu_state.menu.settings_engine_pct as f32 / 100.0);
                    self.settings.fetch_orbital_params = menu_state.menu.settings_fetch_orbital;

                    // Check for pending model loads
                    menu_state
                        .menu
                        .poll_preview_load(&gpu.device);

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
                            &[],
                        );
                    }

                    // Render egui overlay on top
                    {
                        let MenuStateWrapper { egui, menu, .. } = &mut menu_state;
                        egui.render_to_surface(gpu, &surface_view, |ctx| {
                            menu.draw_ui(ctx);
                        });
                    }

                    output.present();

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
                let action = flying.handle_event(gpu, &event);

                match action {
                    FlyingAction::ReturnToMenu => {
                        // Find the index of the current aircraft in cached profiles
                        let prev_idx = self.ensure_profiles()
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
                        if let Some(ref mut player) = self.music_player {
                            player.tick();
                        }
                        let mut t = self.shared_telemetry.lock().unwrap();
                        let s = flying.telemetry_snapshot();
                        // Preserve app_state, aircraft_name, fps — update flight data
                        t.airspeed_kts = s.airspeed_kts;
                        t.groundspeed_kts = s.groundspeed_kts;
                        t.vertical_speed_fpm = s.vertical_speed_fpm;
                        t.altitude_msl_ft = s.altitude_msl_ft;
                        t.altitude_agl_ft = s.altitude_agl_ft;
                        t.heading_deg = s.heading_deg;
                        t.pitch_deg = s.pitch_deg;
                        t.bank_deg = s.bank_deg;
                        t.throttle_pct = s.throttle_pct;
                        t.alpha_deg = s.alpha_deg;
                        t.on_ground = s.on_ground;
                        t.brakes = s.brakes;
                        t.latitude = s.latitude;
                        t.longitude = s.longitude;
                        t.radio_log = s.radio_log;
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
