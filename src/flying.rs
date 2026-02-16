use glam::{DVec3, Quat};
use std::path::Path;
use std::time::Instant;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Fullscreen;

use crate::audio;
use crate::camera::Camera;
use crate::renderer::Renderer;
use crate::settings::SharedVolume;
use crate::{
    ai_traffic, aircraft_profile, airport_gen, airport_markers, atc, celestial,
    earth, physics, scene, sim, telemetry, tle, tts,
};
use crate::{EguiContext, GpuContext, TARGET_FRAME_TIME};

pub struct FlyingState {
    pub renderer: Renderer,
    pub camera: Camera,
    pub objects: Vec<scene::SceneObject>,
    pub last_frame: Instant,
    pub cursor_grabbed: bool,
    pub sim_runner: sim::SimRunner,
    pub aircraft_idx: usize,
    pub model_to_body: Quat,
    pub aircraft_name: String,
    pub earth_renderer: earth::EarthRenderer,
    pub earth_idx: usize,
    pub ai_traffic: ai_traffic::AiTrafficManager,
    pub atc_manager: atc::AtcManager,
    pub atc_states: Vec<atc::types::AiPlaneAtcState>,
    pub egui: EguiContext,
    pub tts_engine: Option<tts::TtsEngine>,
    pub celestial: celestial::CelestialEngine,
    pub celestial_indices: [usize; 5],
    pub airport_markers: Option<airport_markers::AirportMarkers>,
    pub marker_base_idx: usize,
    pub engine_sound: Option<audio::EngineSoundPlayer>,
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
        profile: Option<&aircraft_profile::AircraftProfile>,
        parsed_airports: &mut Option<airport_gen::ParsedAirports>,
        epoch_unix: Option<f64>,
        no_tts: bool,
        atc_volume: Option<SharedVolume>,
        engine_volume: SharedVolume,
        fetch_orbital_params: bool,
    ) -> Self {
        let (params, aircraft_name, obj_path, wingspan, pilot_eye) = match profile {
            Some(p) => (
                p.to_aircraft_params(),
                p.name.clone(),
                if p.has_model() {
                    Some(p.obj_path())
                } else {
                    None
                },
                p.physics.wing_span,
                p.pilot_eye_body(),
            ),
            None => {
                log::warn!("No aircraft profiles found, using hardcoded Ki-61");
                (
                    physics::AircraftParams::ki61(),
                    "Ki-61 Hien".to_string(),
                    None,
                    12.0,
                    DVec3::new(2.0, 0.0, -1.0),
                )
            }
        };

        let renderer = Renderer::new(
            &gpu.device,
            gpu.surface_format,
            gpu.config.width,
            gpu.config.height,
        );

        // Extract orbit spec
        let mut orbit_spec = profile.and_then(|p| p.orbit.clone());

        // Fetch live TLE for orbital vehicles with a NORAD ID (if enabled)
        if fetch_orbital_params {
            if let Some(orbit) = &mut orbit_spec {
                if let Some(norad_id) = orbit.norad_id {
                    if orbit.lagrange_point.is_none() {
                        tle::fetch_and_apply_tle(norad_id, orbit);
                    }
                }
            }
        }

        let mut camera = Camera::new(gpu.config.width as f32 / gpu.config.height as f32);
        if let Some(orbit) = &orbit_spec {
            camera.pitch = orbit.camera_pitch_deg.to_radians();
            if let Some(fov) = orbit.fov_deg {
                camera.fov_deg = fov as f32;
            }
        }

        // Physics setup — orbit or ground start
        let start_jd = celestial::time::unix_to_jd(
            epoch_unix.unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs_f64()
            }),
        );
        let aircraft_body = match &orbit_spec {
            Some(orbit) if orbit.lagrange_point.is_some() => {
                physics::create_at_lagrange_point(orbit.altitude_km, start_jd)
            }
            Some(orbit) => physics::create_from_orbit(orbit, start_jd),
            None => physics::create_aircraft_at_sfo(),
        };
        let simulation = physics::Simulation::new(params, aircraft_body);
        let sim_runner = sim::SimRunner::new(simulation, pilot_eye);

        // Scene setup
        let t0 = Instant::now();
        let mut objects = scene::load_scene(&gpu.device);
        log::info!("[init] load_scene: {:.0}ms", t0.elapsed().as_millis());

        // Use pre-parsed airport data
        let parsed = parsed_airports.take().unwrap_or_else(|| {
            let json = std::fs::read_to_string("assets/airports/airports_all.json")
                .unwrap_or_default();
            airport_gen::parse_airports_json(&json)
        });

        // Airport generation
        let t2 = Instant::now();
        let next_id = objects.iter().map(|o| o.object_id).max().unwrap_or(0) + 1;
        let ref_ecef = sim_runner.render_state().pos_ecef;
        let (airport_objects, next_id) =
            airport_gen::generate_airports(&gpu.device, &parsed, next_id, ref_ecef);
        objects.extend(airport_objects);
        log::info!("[init] generate_airports: {:.0}ms", t2.elapsed().as_millis());

        // Earth mesh
        let t3 = Instant::now();
        let (earth_renderer, earth_obj) = earth::EarthRenderer::new(&gpu.device);
        objects.push(earth_obj);
        let earth_idx = objects.len() - 1;
        log::info!("[init] earth: {:.0}ms", t3.elapsed().as_millis());

        // Aircraft object
        let t4 = Instant::now();
        let aircraft_obj = match obj_path {
            Some(path) => {
                scene::load_aircraft_from_path(&gpu.device, &path, wingspan, next_id)
            }
            None => scene::load_aircraft_object(&gpu.device, next_id),
        };
        objects.push(aircraft_obj);
        let aircraft_idx = objects.len() - 1;
        log::info!("[init] aircraft: {:.0}ms", t4.elapsed().as_millis());

        // AI traffic
        let t5 = Instant::now();
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
        log::info!(
            "[init] ai_traffic ({}): {:.0}ms",
            ai_traffic.plane_count(),
            t5.elapsed().as_millis()
        );

        let model_to_body = crate::model_to_body_rotation();

        // Celestial engine
        let t6 = Instant::now();
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
        log::info!("[init] celestial: {:.0}ms", t6.elapsed().as_millis());

        // Airport proximity markers — only in orbital mode
        let t7 = Instant::now();
        let next_marker_id = next_celestial_id + 5;
        let mut airport_markers = if orbit_spec.is_some() {
            airport_markers::AirportMarkers::new(&parsed.positions())
        } else {
            None
        };
        let marker_base_idx = objects.len();
        if let Some(markers) = &mut airport_markers {
            let marker_objects =
                markers.create_scene_objects(&gpu.device, next_marker_id, marker_base_idx);
            objects.extend(marker_objects);
        }
        log::info!("[init] markers: {:.0}ms", t7.elapsed().as_millis());

        // Store parsed airports back
        *parsed_airports = Some(parsed);

        // ATC system
        let num_ai = ai_traffic.plane_count();
        let mut atc_manager = atc::AtcManager::new(num_ai);
        log::info!("[init] TOTAL: {:.0}ms", t0.elapsed().as_millis());
        let atc_states: Vec<atc::types::AiPlaneAtcState> = (0..num_ai)
            .map(|i| atc::build_atc_state(i))
            .collect();

        // TTS engine
        let tts_engine = if !no_tts {
            match tts::TtsEngine::new(atc_volume) {
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

        // egui for radio overlay
        let egui = EguiContext::new(gpu);
        {
            let mut style = (*egui.ctx.style()).clone();
            style.visuals.window_fill = egui::Color32::TRANSPARENT;
            style.visuals.panel_fill = egui::Color32::TRANSPARENT;
            style.visuals.override_text_color = Some(egui::Color32::WHITE);
            egui.ctx.set_style(style);
        }

        // Grab cursor
        let _ = gpu
            .window
            .set_cursor_grab(winit::window::CursorGrabMode::Confined)
            .or_else(|_| {
                gpu.window
                    .set_cursor_grab(winit::window::CursorGrabMode::Locked)
            });
        gpu.window.set_cursor_visible(false);

        // Engine sound
        let engine_sound = profile
            .and_then(|p| p.engine_sound.as_ref())
            .and_then(|cat| audio::EngineSoundPlayer::new(cat, engine_volume));

        log::info!(
            "Flying: {} ({} objects loaded)",
            aircraft_name,
            objects.len()
        );

        FlyingState {
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
            egui,
            tts_engine,
            celestial: celestial_engine,
            celestial_indices,
            airport_markers,
            marker_base_idx,
            engine_sound,
        }
    }

    /// Handle a window event. Moved from App::handle_flying_event().
    pub fn handle_event(
        &mut self,
        gpu: &GpuContext,
        event: &WindowEvent,
    ) -> FlyingAction {
        match event {
            WindowEvent::Resized(new_size) => {
                let w = new_size.width.max(1);
                let h = new_size.height.max(1);
                self.renderer.resize(&gpu.device, w, h);
                self.camera.aspect = w as f32 / h as f32;
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
                        self.camera.yaw = 0.0;
                        self.camera.pitch = 0.0;
                        FlyingAction::None
                    }
                    KeyCode::KeyP => {
                        self.celestial.star_toggle =
                            self.celestial.star_toggle.cycle();
                        FlyingAction::None
                    }
                    _ => {
                        self.sim_runner.key_down(*key);
                        FlyingAction::None
                    }
                },
                ElementState::Released => {
                    self.sim_runner.key_up(*key);
                    FlyingAction::None
                }
            },

            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                if !self.cursor_grabbed {
                    self.cursor_grabbed = true;
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
                let dt = now.duration_since(self.last_frame).as_secs_f64();
                self.last_frame = now;

                // Advance physics
                self.sim_runner.update(dt);

                // Get interpolated render state
                let render_state = self.sim_runner.render_state();

                // Update camera position to pilot eye
                self.camera.position =
                    self.sim_runner.camera_position(&render_state);

                // Update clip planes and earth mesh for current altitude
                let altitude_m = self.sim_runner.sim.aircraft.lla.alt;
                self.camera.update_clip_planes(altitude_m);

                // View matrix from aircraft orientation + pilot head look
                let view = sim::aircraft_view_matrix(
                    render_state.orientation,
                    self.camera.yaw,
                    self.camera.pitch,
                );
                let proj = self.camera.projection_matrix();

                self.earth_renderer.update(
                    &gpu.device,
                    &gpu.queue,
                    &mut self.objects[self.earth_idx],
                    self.camera.position,
                    altitude_m,
                );

                // Update aircraft SceneObject from physics
                let aircraft = &mut self.objects[self.aircraft_idx];
                aircraft.world_pos = render_state.pos_ecef;
                aircraft.rotation =
                    sim::dquat_to_quat(render_state.orientation) * self.model_to_body;

                // Update celestial bodies
                self.celestial.update(dt, self.camera.position);
                self.celestial.update_scene_objects(
                    &gpu.device,
                    &gpu.queue,
                    &mut self.objects,
                    &self.celestial_indices,
                    self.camera.position,
                    altitude_m,
                    self.camera.far,
                );

                // Update AI traffic
                self.ai_traffic.update(dt);
                let ai_count = self.ai_traffic.plane_count();
                for i in 0..ai_count {
                    let idx = self.ai_traffic.scene_indices()[i];
                    let pos = self.ai_traffic.planes()[i].pos_ecef;
                    let orient = self.ai_traffic.planes()[i].orientation;
                    self.objects[idx].world_pos = pos;
                    self.objects[idx].rotation =
                        sim::dquat_to_quat(orient) * self.model_to_body;
                }

                // Update airport proximity markers
                if let Some(markers) = &mut self.airport_markers {
                    markers.update(dt, self.camera.position, &mut self.objects);
                }

                // ATC tick
                self.atc_manager.advance_enroute_timers(dt);
                self.atc_manager.tick(
                    dt,
                    self.ai_traffic.planes(),
                    &mut self.atc_states,
                    render_state.pos_ecef,
                );

                // Cull invisible objects
                const DISTANCE_CULL_M: f64 = 400_000.0;
                let culled: Vec<(usize, u32)> = self
                    .objects
                    .iter()
                    .enumerate()
                    .filter_map(|(i, obj)| {
                        if i == self.earth_idx
                            || i == self.aircraft_idx
                            || self.celestial_indices.contains(&i)
                            || (i >= self.marker_base_idx
                                && i < self.marker_base_idx + 1024)
                        {
                            return None;
                        }
                        let rel = obj.world_pos - self.camera.position;
                        let dist = rel.length();
                        let radius = obj.bounding_radius as f64;
                        if dist - radius > DISTANCE_CULL_M {
                            return Some((i, obj.index_count));
                        }
                        let rel_f32 = rel.as_vec3();
                        let view_z = view.transform_point3(rel_f32).z;
                        if view_z > obj.bounding_radius {
                            return Some((i, obj.index_count));
                        }
                        None
                    })
                    .collect();
                for &(i, _) in &culled {
                    self.objects[i].index_count = 0;
                }

                // Render
                let output = match gpu.surface.get_current_texture() {
                    Ok(t) => t,
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        for (i, count) in culled {
                            self.objects[i].index_count = count;
                        }
                        return FlyingAction::ReconfigureSurface;
                    }
                    Err(e) => {
                        log::error!("Surface error: {}", e);
                        for (i, count) in culled {
                            self.objects[i].index_count = count;
                        }
                        return FlyingAction::None;
                    }
                };

                let surface_view = output
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                // Solid overlay: sun + airport markers
                let sun_idx = self.celestial_indices[0];
                let mut overlay_indices = vec![sun_idx];
                for i in 0..1024 {
                    let idx = self.marker_base_idx + i;
                    if idx < self.objects.len()
                        && self.objects[idx].index_count > 0
                    {
                        overlay_indices.push(idx);
                    }
                }
                self.renderer.render(
                    &gpu.device,
                    &gpu.queue,
                    &surface_view,
                    &self.objects,
                    view,
                    proj,
                    self.camera.position,
                    &overlay_indices,
                    &[],
                );

                // Restore culled objects' index counts
                for (i, count) in culled {
                    self.objects[i].index_count = count;
                }

                // egui radio overlay
                let recent = self.atc_manager.recent_messages(15.0);
                let com1 = self.atc_manager.com1_freq;
                self.egui.render_to_surface(gpu, &surface_view, |ctx| {
                    atc::overlay::draw_radio_overlay(ctx, &recent, com1);
                });

                output.present();

                // Update engine sound volume + pitch
                if let Some(ref engine) = self.engine_sound {
                    let throttle = self.sim_runner.sim.controls.throttle as f32;
                    engine.tick(throttle);
                }

                // Frame pacing
                if let Some(remaining) = TARGET_FRAME_TIME.checked_sub(now.elapsed()) {
                    std::thread::sleep(remaining);
                }

                gpu.window.request_redraw();

                FlyingAction::UpdateTelemetry
            }

            _ => FlyingAction::None,
        }
    }

    /// Collect telemetry snapshot.
    pub fn telemetry_snapshot(&self) -> telemetry::Telemetry {
        let sim = &self.sim_runner.sim;
        let instruments = physics::FlightInstruments::from_aircraft(&sim.aircraft);
        let c = &sim.controls;

        let radio_log: Vec<telemetry::RadioLogEntry> = self
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

        let mut t = telemetry::Telemetry::default();
        t.airspeed_kts = instruments.airspeed_kts;
        t.groundspeed_kts = instruments.groundspeed_kts;
        t.vertical_speed_fpm = instruments.vertical_speed_fpm;
        t.altitude_msl_ft = instruments.altitude_msl_ft;
        t.altitude_agl_ft = instruments.altitude_agl_ft;
        t.heading_deg = instruments.heading_deg;
        t.pitch_deg = instruments.pitch_deg;
        t.bank_deg = instruments.bank_deg;
        t.throttle_pct = c.throttle * 100.0;
        t.alpha_deg = instruments.alpha_deg;
        t.on_ground = instruments.on_ground;
        t.brakes = c.brakes > 0.0;
        t.latitude = instruments.latitude_deg;
        t.longitude = instruments.longitude_deg;
        t.radio_log = radio_log;
        t
    }
}
