use glam::{DMat4, DVec3, Mat4};
use std::collections::HashSet;
use winit::keyboard::KeyCode;

pub struct Camera {
    pub position: DVec3,
    pub yaw: f64,   // radians, 0 = looking along +Z
    pub pitch: f64,  // radians, clamped to [-89, 89] degrees
    pub fov_deg: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
    pub speed: f64,
    pub mouse_sensitivity: f64,
    keys_held: HashSet<KeyCode>,
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        Self {
            position: DVec3::new(0.0, 0.3, -1.5),
            yaw: 0.0,
            pitch: 0.0,
            fov_deg: 115.0,
            aspect,
            near: 1.0,
            far: 40000.0,
            speed: 10.0,
            mouse_sensitivity: 0.003,
            keys_held: HashSet::new(),
        }
    }

    pub fn key_down(&mut self, key: KeyCode) {
        self.keys_held.insert(key);
    }

    pub fn key_up(&mut self, key: KeyCode) {
        self.keys_held.remove(&key);
    }

    pub fn mouse_move(&mut self, dx: f64, dy: f64) {
        self.yaw += dx * self.mouse_sensitivity;
        self.pitch -= dy * self.mouse_sensitivity;
        // Clamp pitch to avoid gimbal lock
        let limit = 89.0_f64.to_radians();
        self.pitch = self.pitch.clamp(-limit, limit);
    }

    pub fn update(&mut self, dt: f64) {
        // Forward = full look direction (including pitch), like Quake noclip
        let forward = DVec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        // Right is always horizontal
        let right = DVec3::new(self.yaw.cos(), 0.0, -self.yaw.sin());
        let up = DVec3::Y;

        let mut move_dir = DVec3::ZERO;

        if self.keys_held.contains(&KeyCode::KeyW) {
            move_dir += forward;
        }
        if self.keys_held.contains(&KeyCode::KeyS) {
            move_dir -= forward;
        }
        if self.keys_held.contains(&KeyCode::KeyD) {
            move_dir += right;
        }
        if self.keys_held.contains(&KeyCode::KeyA) {
            move_dir -= right;
        }
        if self.keys_held.contains(&KeyCode::Space) {
            move_dir += up;
        }
        if self.keys_held.contains(&KeyCode::ShiftLeft)
            || self.keys_held.contains(&KeyCode::ControlLeft)
        {
            move_dir -= up;
        }

        if move_dir.length_squared() > 0.0 {
            move_dir = move_dir.normalize();
        }

        self.position += move_dir * self.speed * dt;
    }

    pub fn view_matrix(&self) -> Mat4 {
        // Build view matrix: look direction from yaw + pitch
        let dir = DVec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        let target = self.position + dir;
        let view = DMat4::look_at_rh(self.position, target, DVec3::Y);
        // Cast to f32 for GPU
        let cols = view.to_cols_array();
        Mat4::from_cols_array(&cols.map(|v| v as f32))
    }

    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fov_deg.to_radians(), self.aspect, self.near, self.far)
    }

    /// Returns view matrix that has camera at origin (for camera-relative rendering)
    pub fn view_matrix_at_origin(&self) -> Mat4 {
        let dir = DVec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        // View matrix looking from origin
        let view = DMat4::look_at_rh(DVec3::ZERO, dir, DVec3::Y);
        let cols = view.to_cols_array();
        Mat4::from_cols_array(&cols.map(|v| v as f32))
    }
}
