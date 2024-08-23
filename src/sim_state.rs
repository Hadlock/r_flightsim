use macroquad::prelude::*;

pub struct SimState {
    pub draw_objects: bool,
    pub gridspacing: f32,
    pub position: Vec3,
    pub rotation_angle: f32,
    pub plane_position: Vec3,
    pub throttle: bool,
    pub speed: f32,
    pub x: f32,
    pub switch: bool,
    pub bounds: f32,
    pub world_up: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub front: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub last_mouse_position: Vec2,
    pub grabbed: bool,
}

impl SimState {
    pub fn new() -> Self {
        let yaw: f32 = 1.18;
        let pitch: f32 = 0.0;
        let front = vec3(
            yaw.cos() * pitch.cos(),
            pitch.sin(),
            yaw.sin() * pitch.cos(),
        )
        .normalize();
        let world_up = vec3(0.0, 1.0, 0.0);
        let right = front.cross(world_up).normalize();

        SimState {
            draw_objects: true,
            gridspacing: 1.0,
            position: vec3(0.0, 1.0, 0.0), // camera position
            rotation_angle: 0.0,
            plane_position: vec3(-5.0, 0.0, 0.0),
            throttle: false,
            speed: 0.0,
            x: 0.0,
            switch: false,
            bounds: 8.0,
            world_up,
            yaw,
            pitch,
            front,
            right,
            up: Default::default(),
            last_mouse_position: mouse_position().into(),
            grabbed: true,
        }
    }
}

// Assuming mouse_position() is defined somewhere in your project
fn mouse_position() -> Vec2 {
    // Dummy implementation, replace with actual implementation
    Vec2::new(0.0, 0.0)
}