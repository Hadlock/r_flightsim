use macroquad::prelude::*;
use crate::consts::{MOVE_SPEED, LOOK_SPEED};

pub fn handle_input(
    draw_objects: &mut bool,
    grabbed: &mut bool,
    position: &mut Vec3,
    last_mouse_position: &mut Vec2,
    yaw: &mut f32,
    pitch: &mut f32,
    front: &mut Vec3,
    right: &mut Vec3,
    up: &mut Vec3,
    x: &mut f32,
    switch: &mut bool,
    throttle: &mut bool,
    bounds: f32,
    delta: f32,
    world_up: Vec3,
) -> Vec2 { // Return Vec2

    // probably pass this all in as a giant game state object

    /* #region keyboard input handling */
    if is_key_pressed(KeyCode::Escape) {
        std::process::exit(0);
    }
    if is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl) {
        if is_key_pressed(KeyCode::C) {
            std::process::exit(0);
        }
    }
    if is_key_pressed(KeyCode::P) {
        *draw_objects = !*draw_objects;
    }
    if is_key_pressed(KeyCode::T) {
        *throttle = !*throttle;
    }

    if is_key_pressed(KeyCode::Tab) {
        *grabbed = !*grabbed;
        set_cursor_grab(*grabbed);
        show_mouse(!*grabbed);
    }
    if is_key_down(KeyCode::W) {
        *position += *front * MOVE_SPEED;
    }
    if is_key_down(KeyCode::A) {
        *position -= *right * MOVE_SPEED;
    }
    if is_key_down(KeyCode::S) {
        *position -= *front * MOVE_SPEED;
    }
    if is_key_down(KeyCode::D) {
        *position += *right * MOVE_SPEED;
    }

    let (mouse_x, mouse_y) = mouse_position();
    let mouse_position: Vec2 = vec2(mouse_x, mouse_y);
    let mouse_delta = mouse_position - *last_mouse_position;
    *last_mouse_position = mouse_position;
    /* #endregion */

    /* #region mouse input handling */
    *yaw += mouse_delta.x * delta * LOOK_SPEED;
    *pitch += mouse_delta.y * delta * -LOOK_SPEED;

    *pitch = if *pitch > 1.5 { 1.5 } else { *pitch };
    *pitch = if *pitch < -1.5 { -1.5 } else { *pitch };

    *front = vec3(
        yaw.cos() * pitch.cos(),
        pitch.sin(),
        yaw.sin() * pitch.cos(),
    )
    .normalize();

    *right = front.cross(world_up).normalize();
    *up = right.cross(*front).normalize();

    *x += if *switch { 0.04 } else { -0.04 };
    if *x >= bounds || *x <= -bounds {
        *switch = !*switch;
    }
    /* #endregion */

    mouse_position // Return mouse_position
}