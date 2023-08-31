use egui_demo_lib;
use macroquad::prelude::*;
use macroquad::telemetry; //let _z = telemetry::ZoneGuard::new("input handling");
mod consts;
mod logo;

fn conf() -> Conf {
    Conf {
        window_title: String::from("r_flightsim6"),
        window_width: 1260,
        window_height: 768,
        fullscreen: false,
        ..Default::default()
    }
}

struct Plane {
    F: f64,
    P: f64,
    /* Angle calculations. */
    compassRadians: f64,
    forwardTiltRadians: f64,
    sideTiltRadians: f64,

    /* Next 9 values make up a rotation matrix for a camera transform. See wiki1. */
    R11: f64,
    R12: f64,
    R13: f64,

    R21: f64,
    R22: f64,
    R23: f64,

    R31: f64,
    R32: f64,
    R33: f64,

    /* Things */
    cos_sideTilt: f64,
    sin_sideTilt: f64,
    sin_compass: f64,

}

#[macroquad::main(conf)]
async fn main() {
    //egui_logger::init().unwrap();
    logo::logo();
    /* #region egui stuff 1 of 3 */
    let mut show_egui_demo_windows = false;
    let mut egui_demo_windows = egui_demo_lib::DemoWindows::default();
    /* #endregion */

    /* #region chad stuff */
    let mut gridspacing = 1.0;
    let mut plane_position = vec3(0., 0.5, 0.);
    let mut throttle = false;
    let mut speed = 0.0;
    /* #endregion */

    /* #region cbanks vars */
    let mut D: f64 = 1.0;
    let mut worldZ: f64 = 999.;
    let mut speed: f64 = 8.;
    let mut worldY: f64 = 999.;

    let mut worldX: f64 = 999.;
    let mut forwardTiltRadians: f64 = 0.033;
    let mut airplaneZ: f64 = 1000.;
    let mut S: f64 =74.5;
    let mut speedFeet: f64 = 221.;
    let mut X: f64 = 7.26;

    let gravityAccel: f64 = 32.2;
    let mut cos_sideTilt: f64 = 1.;



    /* #endregion */

    /* #region normal stuff */
    let mut x = 0.0;
    let mut switch = false;
    let bounds = 8.0;

    let world_up = vec3(0.0, 1.0, 0.0);
    let mut yaw: f32 = 1.18;
    let mut pitch: f32 = 0.0;

    let mut front = vec3(
        yaw.cos() * pitch.cos(),
        pitch.sin(),
        yaw.sin() * pitch.cos(),
    )
    .normalize();
    let mut right = front.cross(world_up).normalize();
    let mut up;

    let mut position = vec3(0.0, 1.0, 0.0); //camera position
    let mut last_mouse_position: Vec2 = mouse_position().into();

    let mut grabbed = true;
    set_cursor_grab(grabbed);
    show_mouse(false);
    /* #endregion */

    /* #region another egui */
    let mut pixels_per_point: Option<f32> = None;
    /* #endregion */

    loop {
        let delta = get_frame_time();
        let timeDelta = delta; // = || { get_frame_time()}; //this should be the same as delta, using for cbanks naming parity

        /* #region all input handling */
        let _z = telemetry::ZoneGuard::new("input handling");
        /* #region keyboard input handling */
        if is_key_pressed(KeyCode::T) {
            throttle = !throttle;
        }

        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        if is_key_pressed(KeyCode::Tab) {
            grabbed = !grabbed;
            set_cursor_grab(grabbed);
            show_mouse(!grabbed);
        }
        if is_key_down(KeyCode::W) {
            position += front * consts::MOVE_SPEED;
        }
        if is_key_down(KeyCode::A) {
            position -= right * consts::MOVE_SPEED;
        }
        if is_key_down(KeyCode::S) {
            position -= front * consts::MOVE_SPEED;
        }
        if is_key_down(KeyCode::D) {
            position += right * consts::MOVE_SPEED;
        }

        let mouse_position: Vec2 = mouse_position().into();
        let mouse_delta = mouse_position - last_mouse_position;
        last_mouse_position = mouse_position;
        /* #endregion */

        /* #region mouse input handling */
        yaw += mouse_delta.x * delta * consts::LOOK_SPEED;
        pitch += mouse_delta.y * delta * -consts::LOOK_SPEED;

        pitch = if pitch > 1.5 { 1.5 } else { pitch };
        pitch = if pitch < -1.5 { -1.5 } else { pitch };

        front = vec3(
            yaw.cos() * pitch.cos(),
            pitch.sin(),
            yaw.sin() * pitch.cos(),
        )
        .normalize();

        right = front.cross(world_up).normalize();
        up = right.cross(front).normalize();

        x += if switch { 0.04 } else { -0.04 };
        if x >= bounds || x <= -bounds {
            switch = !switch;
        }
        /* #endregion */

        /* #endregion */
        clear_background(consts::FSBLUE);

        let plane0 = Plane{
            F: timeDelta * P,
            compassRadians: cos_sideTilt * timeDelta * F / 
                            cos_forwardTilt + d /
                            cos_forwardTilt + sinSideTilt * timeDelta,
            forwardTiltRadians: d * timeDelta * cos_sideTilt - timeDelta * F * sin_sideTilt,


        };


        /* #region egui 2 of 3 */
        egui_macroquad::ui(|egui_ctx| {
            if pixels_per_point.is_none() {
                pixels_per_point = Some(egui_ctx.pixels_per_point());
            }

            if show_egui_demo_windows {
                egui_demo_windows.ui(egui_ctx);
            }

            egui::Window::new("r_flightsim6 additional").show(egui_ctx, |ui| {
                ui.checkbox(&mut show_egui_demo_windows, "Show egui demo windows");

                let response = ui.add(
                    egui::Slider::new(pixels_per_point.as_mut().unwrap(), 0.75..=3.0)
                        .logarithmic(true),
                );

                // Don't change scale while dragging the slider
                if response.drag_released() {
                    egui_ctx.set_pixels_per_point(pixels_per_point.unwrap());
                }
            });
            //egui_logger::logger_ui(ui);
            //egui::Window::new("Log").show(egui_ctx, |ui| {egui_logger.logger_ui(ui);});
        });
        /* #endregion */

        // Going 3d!

        set_camera(&Camera3D {
            position: position,
            up: up,
            target: position + front,
            ..Default::default()
        });

        /* #region draw grid */
        //draw stuff
        if position[1] < 5.0 {
            gridspacing = 1.0;
        }
        if position[1] > 5.0 {
            gridspacing = 10.0;
        }
        if position[1] > 10.0 {
            gridspacing = 20.0;
        }
        if position[1] > 30.0 {
            gridspacing = 50.0;
        }

        draw_grid(100, gridspacing, GRAY, WHITE); //(primary x/y), (grid)
                                                  /* #endregion */

        //draw_line_3d(
        //    vec3(x, 0.0, x),
        //    vec3(5.0, 5.0, 5.0),
        //    Color::new(1.0, 1.0, 0.0, 1.0),
        //);

        /* #region draw airplane */
        fn draw_airplane(plane_position: Vec3, color: Color) {
            draw_cube_wires(plane_position, vec3(1., 1., 1.), color); //position, size
        }

        draw_airplane(plane_position, ORANGE);
        if throttle {
            speed += 0.01;
        };
        if !throttle {
            if speed > 0.0 {
                speed -= 0.01;
            }
        }
        /* #endregion */

        /* #region handle airplane speed and direction */
        if speed > 0.0 {
            plane_position[0] += speed;
        }

        if is_key_down(KeyCode::Right) {
            plane_position[2] += speed * 0.12;
        }
        if is_key_down(KeyCode::Left) {
            plane_position[2] -= speed * 0.12;
        }
        if speed > 0.5 {
            plane_position[1] += 0.5;
        }
        if speed < 0.5 {
            if plane_position[1] > 0.0 {
                plane_position[1] -= 1.0;
            }
        }
        /* #endregion */

        // Back to screen space, render some text

        set_default_camera();

        /* #region draw text */
        draw_text("First Person Camera", 10.0, 20.0, 30.0, WHITE);

        draw_text(
            format!("X: {} Y: {}", mouse_position.x, mouse_position.y).as_str(),
            10.0,
            48.0 + 18.0,
            30.0,
            WHITE,
        );
        draw_text(
            format!("Press <TAB> to toggle mouse grab: {}", grabbed).as_str(),
            10.0,
            48.0 + 42.0,
            30.0,
            WHITE,
        );
        /* #endregion */

        // draw profiler
        if consts::PROFILER {
            macroquad_profiler::profiler(Default::default());
        }

        //draw egui on top 3 of 3
        egui_macroquad::draw();

        next_frame().await
    }
}
