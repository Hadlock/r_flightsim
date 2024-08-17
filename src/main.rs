mod consts;
mod logo;
mod draw_objects;
mod draw_models;
mod input_handling;
mod load_assets;

use input_handling::handle_input;
use load_assets::{Assets, BoundingBox, calculate_aabb, check_collision};
use macroquad::prelude::*;
use macroquad::{telemetry}; 
use draw_models::draw_models;

fn conf() -> Conf {
    Conf {
        window_title: String::from("r_flightsim7"),
        window_width: 1280,
        window_height: 768,
        fullscreen: false,
        ..Default::default()
    }
}

#[macroquad::main(conf)]
async fn main() {
    logo::logo(); 

    // Load assets
    let assets = load_assets::load_assets().await;

    // Introduce a boolean variable to keep track of the toggle state
    let mut draw_objects = true;
    let mut gridspacing = 1.0;

    let mut position = vec3(0.0, 1.0, 0.0); //camera position

    // Rotation angle
    let mut rotation_angle: f32 = 0.0;

    // ok, pulling in camera control stuff
    /* #region chad stuff */
    let mut gridspacing = 1.0;
    let mut plane_position = vec3(0., 0.5, 0.);
    let mut throttle = false;
    let mut speed = 0.0;
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
    let mut up = Default::default();

    let mut position = vec3(0.0, 1.0, 0.0); //camera position
    let mut last_mouse_position: Vec2 = mouse_position().into();

    let mut grabbed = true;
    set_cursor_grab(grabbed);
    show_mouse(false);
    /* #endregion */


    loop {
        let delta = get_frame_time();

        let mouse_position = handle_input(
            &mut draw_objects,
            &mut grabbed,
            &mut position,
            &mut last_mouse_position,
            &mut yaw,
            &mut pitch,
            &mut front,
            &mut right,
            &mut up,
            &mut x,
            &mut switch,
            bounds,
            delta,
            world_up,
        );


        clear_background(consts::FSBLUE);

        // Going 3d!
        set_camera(&Camera3D {
            position: position,
            up: up,
            target: position + front,
            ..Default::default()
        });

        // grid
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
        if position[1] > 70.0 {
            gridspacing = 100.0;
        }
        draw_grid(100, gridspacing, GRAY, WHITE);
        //end grid

        // Conditionally draw the objects based on the value of draw_objects
        if draw_objects {
            draw_objects::draw_objects(&assets.rust_logo, &assets.ferris).await;
        }
        // Draw the models
        draw_models(rotation_angle, &assets.vertices1, &assets.vertices2, &assets.mesh1, &assets.mesh2);

        //draw_text("First Person Camera", 10.0, 20.0, 30.0, WHITE);
        if check_collision(&assets.bbox1, &assets.bbox2) {
            println!("Collision detected!");
        }

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

        // Calculate the altitude via x-coordinate for the top right corner and draw the text
        let altitude = position[1].round();
        let text = if altitude > 18000.0 {
            // do crazy flight level stuff to be fancy
            format!("FL{:03}", (altitude / 100.0).round() as i32)
        } else {
            format!("alt: {}", altitude)
        };
        let x = screen_width() - measure_text(&text, None, 30, 1.0).width - 10.0;
        draw_text(&text, x, 20.0, 30.0, WHITE);   

        /* #endregion */

        // Increment the rotation angle
        rotation_angle += 1.0;
        macroquad_profiler::profiler(Default::default());
        next_frame().await; 
    }
}
