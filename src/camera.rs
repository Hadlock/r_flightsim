use glam::{DVec3, Mat4};

pub struct Camera {
    pub position: DVec3,
    pub yaw: f64,   // radians, 0 = looking forward, positive = look right
    pub pitch: f64,  // radians, 0 = level, positive = look up
    pub fov_deg: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
    pub mouse_sensitivity: f64,
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        Self {
            position: DVec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            fov_deg: 115.0,
            aspect,
            near: 1.0,
            far: 40000.0,
            mouse_sensitivity: 0.003,
        }
    }

    pub fn mouse_move(&mut self, dx: f64, dy: f64) {
        self.yaw -= dx * self.mouse_sensitivity;
        self.pitch -= dy * self.mouse_sensitivity;
        let pitch_limit = 89.0_f64.to_radians();
        let yaw_limit = 150.0_f64.to_radians();
        self.pitch = self.pitch.clamp(-pitch_limit, pitch_limit);
        self.yaw = self.yaw.clamp(-yaw_limit, yaw_limit);
    }

    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fov_deg.to_radians(), self.aspect, self.near, self.far)
    }
}
