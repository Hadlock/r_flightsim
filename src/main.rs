use macroquad::prelude::*;
use tobj::{self, LoadOptions};
use std::path::Path;

mod consts;
mod logo;

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

    let rust_logo = load_texture("src/rust.png").await.unwrap();
    let ferris = load_texture("src/ferris.png").await.unwrap();

    // Load the OBJ file with LoadOptions
    let load_options = LoadOptions {
        triangulate: true,
        single_index: true,
        ..Default::default()
    };
    let (models, _materials) = tobj::load_obj(&Path::new("src/obj/teapot.obj"), &load_options).expect("Failed to load OBJ file");

    // Extract vertices from the first model (assuming the OBJ file contains only one model)
    let mesh = &models[0].mesh;
    let vertices: Vec<Vec3> = mesh.positions.chunks(3).map(|chunk| vec3(chunk[0], chunk[1], chunk[2])).collect();

    // Rotation angle
    let mut rotation_angle: f32 = 0.0;

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
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

        draw_cube_wires(vec3(0., 1., -6.), vec3(2., 2., 2.), DARKGREEN);
        draw_cube_wires(vec3(0., 1., 6.), vec3(2., 2., 2.), DARKBLUE);
        draw_cube_wires(vec3(2., 1., 2.), vec3(2., 2., 2.), YELLOW);

        draw_plane(vec3(-8., 0., -8.), vec2(5., 5.), Some(&ferris), WHITE);

        draw_cube(
            vec3(-5., 1., -2.),
            vec3(2., 2., 2.),
            Some(&rust_logo),
            WHITE,
        );
        draw_cube(vec3(-5., 1., 2.), vec3(2., 2., 2.), Some(&ferris), WHITE);

        // Create a rotation matrix
        let rotation_matrix = Mat4::from_rotation_y(rotation_angle.to_radians());

        // Create a translation matrix
        let translation_matrix = Mat4::from_translation(vec3(5.0, 0.0, 0.0));

        // Combine the rotation and translation matrices
        let transformation_matrix = translation_matrix * rotation_matrix;

        // Draw the OBJ model with rotation and translation
        for i in (0..mesh.indices.len()).step_by(3) {
            let idx0 = mesh.indices[i] as usize;
            let idx1 = mesh.indices[i + 1] as usize;
            let idx2 = mesh.indices[i + 2] as usize;

            let v0 = transformation_matrix.transform_point3(vertices[idx0]);
            let v1 = transformation_matrix.transform_point3(vertices[idx1]);
            let v2 = transformation_matrix.transform_point3(vertices[idx2]);

            draw_line_3d(v0, v1, BLACK);
            draw_line_3d(v1, v2, BLACK);
            draw_line_3d(v2, v0, BLACK);
        }

        // Increment the rotation angle
        rotation_angle += 1.0;

        next_frame().await;
    }
}