use std::sync::mpsc;

use egui::{Align, Color32, CornerRadius, Layout, RichText, Vec2};

use crate::aircraft_profile::AircraftProfile;
use crate::obj_loader::{self, MeshData};
use crate::scene::{self, SceneObject};

/// FSBLUE color family for egui
const FSBLUE: Color32 = Color32::from_rgb(25, 51, 76);
const FSBLUE_LIGHT: Color32 = Color32::from_rgb(38, 76, 114);
const FSBLUE_DARK: Color32 = Color32::from_rgb(15, 30, 46);
const FSBLUE_ACCENT: Color32 = Color32::from_rgb(51, 102, 153);

#[derive(PartialEq, Clone, Copy)]
pub enum MenuTab {
    PlaneSelect,
    AirportSelect,
    WeatherSelect,
    Settings,
}

pub struct MenuState {
    pub profiles: Vec<AircraftProfile>,
    pub selected_index: usize,
    pub active_tab: MenuTab,
    pub preview_rotation: f32,
    pub preview_object: Option<SceneObject>,
    pub fly_now_clicked: bool,

    // Preview interaction
    pub preview_zoom: f32,         // 1.0 = default, range [0.7, 1.3]
    pub preview_paused: bool,
    pub preview_pitch: f32,        // radians, clamped to ±60°
    pub preview_yaw_vel: f32,      // rad/s from arrow keys
    pub preview_pitch_vel: f32,    // rad/s from arrow keys

    // Async model loading
    pending_load: Option<mpsc::Receiver<MeshData>>,
    pending_slug: String,
    loaded_slug: String,

    // Settings sliders (0–100 integer percent, step 5)
    pub settings_music_pct: u32,
    pub settings_atc_pct: u32,
    pub settings_engine_pct: u32,
    pub settings_fetch_orbital: bool,
}

impl MenuState {
    pub fn new(
        profiles: Vec<AircraftProfile>,
        music_pct: u32,
        atc_pct: u32,
        engine_pct: u32,
        fetch_orbital: bool,
    ) -> Self {
        Self {
            profiles,
            selected_index: 0,
            active_tab: MenuTab::PlaneSelect,
            preview_rotation: 0.0,
            preview_object: None,
            fly_now_clicked: false,
            preview_zoom: 1.0,
            preview_paused: false,
            preview_pitch: 0.0,
            preview_yaw_vel: 0.0,
            preview_pitch_vel: 0.0,
            pending_load: None,
            pending_slug: String::new(),
            loaded_slug: String::new(),
            settings_music_pct: music_pct,
            settings_atc_pct: atc_pct,
            settings_engine_pct: engine_pct,
            settings_fetch_orbital: fetch_orbital,
        }
    }

    /// Get the currently selected profile, if any.
    pub fn selected_profile(&self) -> Option<&AircraftProfile> {
        self.profiles.get(self.selected_index)
    }

    /// Start loading the preview model for the selected aircraft on a background thread.
    pub fn request_preview_load(&mut self) {
        let profile = match self.profiles.get(self.selected_index) {
            Some(p) => p,
            None => return,
        };

        // Skip if already loaded or loading this slug
        if profile.slug == self.loaded_slug || profile.slug == self.pending_slug {
            return;
        }

        if !profile.has_model() {
            self.preview_object = None;
            self.loaded_slug = profile.slug.clone();
            self.pending_slug.clear();
            return;
        }

        let obj_path = profile.obj_path();
        let (tx, rx) = mpsc::channel();
        self.pending_slug = profile.slug.clone();
        self.pending_load = Some(rx);

        std::thread::spawn(move || {
            let mesh = obj_loader::load_obj(&obj_path);
            let _ = tx.send(mesh);
        });
    }

    /// Check if a pending model load has completed.
    /// If so, upload GPU buffers and create the SceneObject.
    pub fn poll_preview_load(&mut self, device: &wgpu::Device) {
        let rx = match &self.pending_load {
            Some(rx) => rx,
            None => return,
        };

        match rx.try_recv() {
            Ok(mesh) => {
                let profile = &self.profiles[self.selected_index];
                let wingspan = profile.physics.wing_span;
                // Scale to fit preview: normalize to ~10 unit wingspan
                let scale = scene::compute_wingspan_scale(&mesh, wingspan);
                // Additional scale to normalize to ~10 units for preview
                let preview_scale = 10.0 / wingspan as f32;
                let obj = scene::create_scene_object(
                    device,
                    &mesh,
                    "preview",
                    scale * preview_scale,
                    1,
                );
                self.preview_object = Some(obj);
                self.loaded_slug = self.pending_slug.clone();
                self.pending_slug.clear();
                self.pending_load = None;
            }
            Err(mpsc::TryRecvError::Empty) => {
                // Still loading
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                // Load failed
                self.pending_slug.clear();
                self.pending_load = None;
            }
        }
    }

    /// Update preview rotation and pitch. Called each frame with dt.
    pub fn update_preview(&mut self, dt: f32) {
        const BASE_SPIN: f32 = 0.5236; // 5 RPM = 30 deg/sec
        const ARROW_ACCEL: f32 = 3.0;  // rad/s² acceleration from arrow keys
        const FRICTION: f32 = 4.0;     // deceleration when no key held
        const MAX_YAW_VEL: f32 = 3.0;  // rad/s max manual yaw speed
        const MAX_PITCH_VEL: f32 = 2.0;
        const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_3; // 60°

        // Apply friction/deceleration to velocities
        let decay = (-FRICTION * dt).exp();
        self.preview_yaw_vel *= decay;
        self.preview_pitch_vel *= decay;

        // Clamp velocities
        self.preview_yaw_vel = self.preview_yaw_vel.clamp(-MAX_YAW_VEL, MAX_YAW_VEL);
        self.preview_pitch_vel = self.preview_pitch_vel.clamp(-MAX_PITCH_VEL, MAX_PITCH_VEL);

        // Auto-spin + manual yaw
        let yaw_rate = if self.preview_paused { 0.0 } else { BASE_SPIN }
            + self.preview_yaw_vel;
        self.preview_rotation += yaw_rate * dt;
        if self.preview_rotation > std::f32::consts::TAU {
            self.preview_rotation -= std::f32::consts::TAU;
        } else if self.preview_rotation < 0.0 {
            self.preview_rotation += std::f32::consts::TAU;
        }

        // Pitch
        self.preview_pitch += self.preview_pitch_vel * dt;
        self.preview_pitch = self.preview_pitch.clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Apply arrow key acceleration. `yaw`: +1 right, -1 left. `pitch`: +1 up, -1 down.
    pub fn apply_arrow_input(&mut self, yaw: f32, pitch: f32, dt: f32) {
        const ARROW_ACCEL: f32 = 3.0;
        self.preview_yaw_vel += yaw * ARROW_ACCEL * dt;
        self.preview_pitch_vel += pitch * ARROW_ACCEL * dt;
    }

    /// Adjust zoom by scroll delta. Positive = zoom in.
    pub fn apply_scroll_zoom(&mut self, delta: f32) {
        self.preview_zoom = (self.preview_zoom + delta * 0.05).clamp(0.7, 1.7);
    }

    /// Draw the egui menu UI. Returns true if "Fly Now" was clicked.
    pub fn draw_ui(&mut self, ctx: &egui::Context) -> bool {
        self.fly_now_clicked = false;

        // Top tab bar
        egui::TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let tabs = [
                    (MenuTab::PlaneSelect, "Plane Select"),
                    (MenuTab::AirportSelect, "Airport Select"),
                    (MenuTab::WeatherSelect, "Weather Select"),
                    (MenuTab::Settings, "Settings"),
                ];
                for (tab, label) in tabs {
                    let selected = self.active_tab == tab;
                    let text = if selected {
                        RichText::new(label).color(Color32::WHITE).strong()
                    } else {
                        RichText::new(label).color(Color32::from_rgb(150, 170, 190))
                    };
                    let btn = ui.add(
                        egui::Button::new(text)
                            .fill(if selected { FSBLUE_ACCENT } else { FSBLUE_DARK })
                            .corner_radius(CornerRadius::same(4))
                            .min_size(Vec2::new(120.0, 30.0)),
                    );
                    if btn.clicked() {
                        self.active_tab = tab;
                    }
                }
            });
        });

        match self.active_tab {
            MenuTab::PlaneSelect => self.draw_plane_select(ctx),
            MenuTab::Settings => self.draw_settings(ctx),
            _ => self.draw_coming_soon(ctx),
        }

        self.fly_now_clicked
    }

    fn draw_plane_select(&mut self, ctx: &egui::Context) {
        let mut new_selection = None;

        // Left panel: aircraft list
        egui::SidePanel::left("aircraft_list")
            .resizable(false)
            .exact_width(180.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, profile) in self.profiles.iter().enumerate() {
                        let selected = i == self.selected_index;
                        let text = if selected {
                            RichText::new(&profile.name)
                                .color(Color32::WHITE)
                                .strong()
                        } else {
                            RichText::new(&profile.name)
                                .color(Color32::from_rgb(180, 195, 210))
                        };

                        let response = ui.add_sized(
                            [ui.available_width(), 28.0],
                            egui::Button::new(text)
                                .fill(if selected { FSBLUE_ACCENT } else { Color32::TRANSPARENT })
                                .corner_radius(CornerRadius::same(3)),
                        );

                        if response.clicked() && !selected {
                            new_selection = Some(i);
                        }
                    }
                });
            });

        if let Some(idx) = new_selection {
            self.selected_index = idx;
            self.request_preview_load();
        }

        // Bottom panel: stats + fly now button
        egui::TopBottomPanel::bottom("stats_panel")
            .min_height(100.0)
            .show(ctx, |ui| {
                let profile = self.profiles.get(self.selected_index).cloned();
                if let Some(profile) = profile {
                    ui.add_space(8.0);

                    // Info line
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&profile.name)
                                .color(Color32::WHITE)
                                .heading(),
                        );
                        ui.label(
                            RichText::new(format!(
                                "  {}  {}  {}",
                                profile.manufacturer, profile.country, profile.year
                            ))
                            .color(Color32::from_rgb(150, 170, 190)),
                        );
                    });
                    ui.label(
                        RichText::new(&profile.description)
                            .color(Color32::from_rgb(130, 150, 170))
                            .italics(),
                    );

                    ui.add_space(4.0);

                    // Stats grid
                    ui.horizontal(|ui| {
                        let stat_order = [
                            "wing_area",
                            "wing_span",
                            "max_thrust",
                            "mass",
                            "max_speed",
                            "range",
                            "ceiling",
                        ];
                        let stat_labels = [
                            "Wing Area",
                            "Wing Span",
                            "Max Thrust",
                            "Mass",
                            "Max Speed",
                            "Range",
                            "Ceiling",
                        ];

                        for (key, label) in stat_order.iter().zip(stat_labels.iter()) {
                            if let Some(value) = profile.stats.get(*key) {
                                ui.vertical(|ui| {
                                    ui.set_width(100.0);
                                    ui.label(
                                        RichText::new(*label)
                                            .color(Color32::from_rgb(120, 140, 160))
                                            .small(),
                                    );
                                    ui.label(
                                        RichText::new(value)
                                            .color(Color32::WHITE)
                                            .strong(),
                                    );
                                });
                            }
                        }
                    });
                }

                // Fly Now button - bottom right
                ui.with_layout(Layout::right_to_left(Align::BOTTOM), |ui| {
                    ui.add_space(12.0);
                    let fly_btn = ui.add_sized(
                        [140.0, 44.0],
                        egui::Button::new(
                            RichText::new("FLY NOW")
                                .color(FSBLUE_DARK)
                                .heading()
                                .strong(),
                        )
                        .fill(Color32::WHITE)
                        .corner_radius(CornerRadius::same(6)),
                    );
                    if fly_btn.clicked() {
                        self.fly_now_clicked = true;
                    }
                    ui.add_space(8.0);
                });
            });

        // Central area is transparent — Sobel preview shows through
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                // Invisible hover sense over the whole preview pane for scroll zoom
                let rect = ui.max_rect();
                let response = ui.interact(rect, ui.id().with("preview_scroll"), egui::Sense::hover());
                if response.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll != 0.0 {
                        self.apply_scroll_zoom(scroll / 30.0);
                    }
                }

                // Loading indicator
                if self.pending_load.is_some() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Loading model...")
                                .color(Color32::from_rgb(150, 170, 190))
                                .heading(),
                        );
                    });
                } else if self.preview_object.is_none() {
                    if let Some(profile) = self.profiles.get(self.selected_index) {
                        if !profile.has_model() {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    RichText::new("No 3D model available")
                                        .color(Color32::from_rgb(120, 140, 160))
                                        .heading(),
                                );
                            });
                        }
                    }
                }
            });
    }

    fn draw_settings(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(24.0);
            ui.heading(RichText::new("Audio").color(Color32::WHITE));
            ui.add_space(12.0);

            let slider_width = 300.0;

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Music Volume")
                        .color(Color32::from_rgb(180, 195, 210)),
                );
                ui.add_space(16.0);
                let mut val = self.settings_music_pct as f32;
                let slider = egui::Slider::new(&mut val, 0.0..=100.0)
                    .step_by(5.0)
                    .suffix("%")
                    .custom_formatter(|v, _| format!("{:.0}", v));
                let response = ui.add_sized([slider_width, 20.0], slider);
                if response.changed() {
                    self.settings_music_pct = val as u32;
                }
            });

            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("ATC Voice Volume")
                        .color(Color32::from_rgb(180, 195, 210)),
                );
                ui.add_space(16.0);
                let mut val = self.settings_atc_pct as f32;
                let slider = egui::Slider::new(&mut val, 0.0..=100.0)
                    .step_by(5.0)
                    .suffix("%")
                    .custom_formatter(|v, _| format!("{:.0}", v));
                let response = ui.add_sized([slider_width, 20.0], slider);
                if response.changed() {
                    self.settings_atc_pct = val as u32;
                }
            });

            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Engine Volume")
                        .color(Color32::from_rgb(180, 195, 210)),
                );
                ui.add_space(16.0);
                let mut val = self.settings_engine_pct as f32;
                let slider = egui::Slider::new(&mut val, 0.0..=100.0)
                    .step_by(5.0)
                    .suffix("%")
                    .custom_formatter(|v, _| format!("{:.0}", v));
                let response = ui.add_sized([slider_width, 20.0], slider);
                if response.changed() {
                    self.settings_engine_pct = val as u32;
                }
            });

            ui.add_space(24.0);
            ui.heading(RichText::new("Orbital").color(Color32::WHITE));
            ui.add_space(12.0);

            ui.checkbox(
                &mut self.settings_fetch_orbital,
                RichText::new("Fetch live orbital parameters")
                    .color(Color32::from_rgb(180, 195, 210)),
            );
            ui.label(
                RichText::new("When enabled, spacecraft profiles fetch live TLE data from CelesTrak for accurate orbital positions.")
                    .color(Color32::from_rgb(120, 140, 160))
                    .small(),
            );
        });
    }

    fn draw_coming_soon(&self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new("Coming Soon")
                            .color(Color32::from_rgb(150, 170, 190))
                            .heading()
                            .size(32.0),
                    );
                });
            });
    }
}

pub fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let visuals = &mut style.visuals;

    visuals.dark_mode = true;
    visuals.override_text_color = Some(Color32::WHITE);

    // Panel backgrounds - semi-transparent FSBLUE
    visuals.panel_fill = Color32::from_rgba_unmultiplied(25, 51, 76, 220);
    visuals.window_fill = Color32::from_rgba_unmultiplied(25, 51, 76, 220);
    visuals.extreme_bg_color = FSBLUE_DARK;
    visuals.faint_bg_color = Color32::from_rgba_unmultiplied(38, 76, 114, 100);

    // Widget colors
    visuals.widgets.noninteractive.bg_fill = FSBLUE_DARK;
    visuals.widgets.inactive.bg_fill = FSBLUE;
    visuals.widgets.hovered.bg_fill = FSBLUE_LIGHT;
    visuals.widgets.active.bg_fill = FSBLUE_ACCENT;

    // Borders
    visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0, Color32::from_rgb(60, 90, 120));
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(60, 90, 120));

    ctx.set_style(style);
}
