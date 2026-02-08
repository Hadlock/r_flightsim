mod camera;
mod coords;
mod obj_loader;
mod physics;
mod renderer;
mod scene;
mod sim;

use camera::Camera;
use glam::Quat;
use renderer::Renderer;
use std::time::Instant;
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

/// OBJ model → body frame rotation.
/// OBJ has X=forward, Y=right, Z=up. Body has X=forward, Y=right, Z=down.
/// 180° around X flips Y and Z: gives (x, -y, -z). This mirrors the model
/// left-right (invisible for symmetric aircraft) and flips Z correctly.
fn model_to_body_rotation() -> Quat {
    Quat::from_rotation_x(std::f32::consts::PI)
}

struct AppState {
    window: Window,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    camera: Camera,
    objects: Vec<scene::SceneObject>,
    last_frame: Instant,
    cursor_grabbed: bool,
    sim_runner: sim::SimRunner,
    aircraft_idx: usize,
    model_to_body: Quat,
}

struct App {
    state: Option<AppState>,
}

impl App {
    fn new() -> Self {
        Self { state: None }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window_attrs = Window::default_attributes()
            .with_title("shaderflight")
            .with_inner_size(PhysicalSize::new(INITIAL_WIDTH, INITIAL_HEIGHT));

        let window = event_loop.create_window(window_attrs).unwrap();

        // wgpu setup
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        // SAFETY: window lives as long as surface (both in AppState)
        let surface = unsafe {
            std::mem::transmute::<wgpu::Surface<'_>, wgpu::Surface<'static>>(
                instance.create_surface(&window).unwrap(),
            )
        };

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
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

        let renderer = Renderer::new(&device, surface_format, width, height);
        let camera = Camera::new(width as f32 / height as f32);

        // --- Physics setup ---
        let aircraft_body = physics::create_aircraft_at_sfo();
        let ref_pos = aircraft_body.pos_ecef;
        let enu = aircraft_body.enu_frame;
        let params = physics::AircraftParams::ki61();
        let simulation = physics::Simulation::new(params, aircraft_body);
        let sim_runner = sim::SimRunner::new(simulation);

        // --- Scene setup ---
        let mut objects = scene::load_scene(&device, ref_pos, &enu);
        let aircraft_obj = scene::load_aircraft_object(&device, 1);
        objects.push(aircraft_obj);
        let aircraft_idx = objects.len() - 1;

        let model_to_body = model_to_body_rotation();

        log::info!(
            "Loaded {} objects, surface format: {:?}",
            objects.len(),
            surface_format
        );

        self.state = Some(AppState {
            window,
            surface,
            device,
            queue,
            config,
            renderer,
            camera,
            objects,
            last_frame: Instant::now(),
            cursor_grabbed: false,
            sim_runner,
            aircraft_idx,
            model_to_body,
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let state = match self.state.as_mut() {
            Some(s) => s,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::Resized(new_size) => {
                let w = new_size.width.max(1);
                let h = new_size.height.max(1);
                state.config.width = w;
                state.config.height = h;
                state.surface.configure(&state.device, &state.config);
                state.renderer.resize(&state.device, w, h);
                state.camera.aspect = w as f32 / h as f32;
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
                    KeyCode::Escape => {
                        if state.cursor_grabbed {
                            state.cursor_grabbed = false;
                            let _ = state
                                .window
                                .set_cursor_grab(winit::window::CursorGrabMode::None);
                            state.window.set_cursor_visible(true);
                        } else {
                            event_loop.exit();
                        }
                    }
                    KeyCode::F11 => {
                        if state.window.fullscreen().is_some() {
                            state.window.set_fullscreen(None);
                        } else {
                            state
                                .window
                                .set_fullscreen(Some(Fullscreen::Borderless(None)));
                        }
                    }
                    KeyCode::KeyC => {
                        state.camera.yaw = 0.0;
                        state.camera.pitch = 0.0;
                    }
                    _ => {
                        state.sim_runner.key_down(key);
                    }
                },
                ElementState::Released => {
                    state.sim_runner.key_up(key);
                }
            },

            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                if !state.cursor_grabbed {
                    state.cursor_grabbed = true;
                    let _ = state
                        .window
                        .set_cursor_grab(winit::window::CursorGrabMode::Confined)
                        .or_else(|_| {
                            state
                                .window
                                .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                        });
                    state.window.set_cursor_visible(false);
                }
            }

            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now.duration_since(state.last_frame).as_secs_f64();
                state.last_frame = now;

                // Advance physics
                state.sim_runner.update(dt);

                // Get interpolated render state
                let render_state = state.sim_runner.render_state();

                // Update camera position to pilot eye
                state.camera.position = state.sim_runner.camera_position(&render_state);

                // View matrix from aircraft orientation + pilot head look
                let view = sim::aircraft_view_matrix(
                    render_state.orientation,
                    state.camera.yaw,
                    state.camera.pitch,
                );
                let proj = state.camera.projection_matrix();

                // Update aircraft SceneObject from physics
                let aircraft = &mut state.objects[state.aircraft_idx];
                aircraft.world_pos = render_state.pos_ecef;
                aircraft.rotation =
                    sim::dquat_to_quat(render_state.orientation) * state.model_to_body;

                // Render
                let output = match state.surface.get_current_texture() {
                    Ok(t) => t,
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        state.surface.configure(&state.device, &state.config);
                        return;
                    }
                    Err(e) => {
                        log::error!("Surface error: {}", e);
                        return;
                    }
                };

                let surface_view = output
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                state.renderer.render(
                    &state.device,
                    &state.queue,
                    &surface_view,
                    &state.objects,
                    view,
                    proj,
                    state.camera.position,
                );

                output.present();
                state.window.request_redraw();
            }

            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        let state = match self.state.as_mut() {
            Some(s) => s,
            None => return,
        };

        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if state.cursor_grabbed {
                state.camera.mouse_move(dx, dy);
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
