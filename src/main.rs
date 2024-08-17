mod consts;
mod logo;
mod draw_objects;
mod draw_models;
mod load_assets;

use load_assets::{Assets, BoundingBox, calculate_aabb, check_collision};
use macroquad::prelude::*;
use draw_models::draw_models;

fn conf() -> Conf {
    Conf {
        window_title: String::from("r_flightsim7"),
        window_width: 1260,
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


    // Rotation angle
    let mut rotation_angle: f32 = 0.0;

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }

        // Check for the 'p' key press to toggle the draw_objects_flag variable
        if is_key_pressed(KeyCode::P) {
            draw_objects = !draw_objects;
        }

        // Handle other key presses
        if is_key_pressed(KeyCode::W) {
            println!("W key pressed");
            // Add your logic for W key press here
        }

        if is_key_pressed(KeyCode::A) {
            println!("A key pressed");
            // Add your logic for A key press here
        }

        if is_key_pressed(KeyCode::S) {
            println!("S key pressed");
            // Add your logic for S key press here
        }

        if is_key_pressed(KeyCode::D) {
            println!("D key pressed");
            // Add your logic for D key press here
        }


        clear_background(consts::FSBLUE);

        // Going 3d!
        set_camera(&Camera3D {
            position: vec3(-20., 15., 0.),
            up: vec3(0., 1., 0.),
            target: vec3(0., 0., 0.),
            ..Default::default()
        });

        
        draw_grid(20, 1., GRAY, WHITE);
        
        // Conditionally draw the objects based on the value of draw_objects
        if draw_objects {
            draw_objects::draw_objects(&assets.rust_logo, &assets.ferris).await;
        }
        
        // Draw the models
        draw_models(rotation_angle, &assets.vertices1, &assets.vertices2, &assets.mesh1, &assets.mesh2);
        
        draw_text("First Person Camera", 10.0, 20.0, 30.0, WHITE);
        if check_collision(&assets.bbox1, &assets.bbox2) {
            println!("Collision detected!");
        }

        // Increment the rotation angle
        rotation_angle += 1.0;

        next_frame().await;
    }
}
