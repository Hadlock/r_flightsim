use macroquad::prelude::*;

struct Plane {
    position: Vec3,
    speed: f32,
    heading: f32,
    altitude: f32,
    up_down: f32,
    left_right: f32,
    throttle: f32,
}

impl Plane {
    fn new() -> Self {
        Plane {
            position: vec3(0.0, 0.0, 100.0),
            speed: 0.0,
            heading: 0.0,
            altitude: 100.0,
            up_down: 0.0,
            left_right: 0.0,
            throttle: 0.0,
        }
    }

    fn update(&mut self, dt: f32) {
        // Update position based on speed, heading, and throttle
        self.position.x += self.speed * self.heading.cos() * dt;
        self.position.y += self.speed * self.heading.sin() * dt;
        self.position.z += self.up_down * dt;

        // Update speed based on throttle
        self.speed += self.throttle * dt;

        // Update altitude
        self.altitude = self.position.z;
    }

    fn handle_input(&mut self) {
        if is_key_down(KeyCode::Up) {
            self.up_down += 1.0;
        }
        if is_key_down(KeyCode::Down) {
            self.up_down -= 1.0;
        }
        if is_key_down(KeyCode::Left) {
            self.left_right += 1.0;
        }
        if is_key_down(KeyCode::Right) {
            self.left_right -= 1.0;
        }
        if is_key_down(KeyCode::PageUp) {
            self.throttle += 1.0;
        }
        if is_key_down(KeyCode::PageDown) {
            self.throttle -= 1.0;
        }
        if is_key_down(KeyCode::Enter) {
            self.left_right = 0.0;
        }
    }
}

#[macroquad::main("Flight Simulator")]
async fn main() {
    let mut plane = Plane::new();

    loop {
        let dt = get_frame_time();

        // Handle input
        plane.handle_input();

        // Update plane state
        plane.update(dt);

        // Clear the screen
        clear_background(BLACK);

        // Draw the plane (simple representation)
        draw_text(
            &format!(
                "Position: x: {:.2}, y: {:.2}, z: {:.2}\nSpeed: {:.2} knots\nHeading: {:.2} degrees\nAltitude: {:.2} feet",
                plane.position.x, plane.position.y, plane.position.z, plane.speed, plane.heading, plane.altitude
            ),
            20.0,
            20.0,
            20.0,
            WHITE,
        );

        // Draw HUD
        draw_text(
            &format!(
                "Speed: {:.2} knots\nHeading: {:.2} degrees\nAltitude: {:.2} feet",
                plane.speed, plane.heading, plane.altitude
            ),
            20.0,
            screen_height() - 60.0,
            20.0,
            WHITE,
        );

        // Next frame
        next_frame().await;
    }
}