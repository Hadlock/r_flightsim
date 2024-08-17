use macroquad::prelude::*;
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


#[macroquad::main(conf)]
async fn main() {
    logo::logo(); 

    let rust_logo = load_texture("src/rust.png").await.unwrap();
    let ferris = load_texture("src/ferris.png").await.unwrap();

    loop {
        clear_background(LIGHTGRAY);

        // Going 3d!

        set_camera(&Camera3D {
            position: vec3(-20., 15., 0.),
            up: vec3(0., 1., 0.),
            target: vec3(0., 0., 0.),
            ..Default::default()
        });

        draw_grid(20, 1., BLACK, GRAY);

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
        draw_cube(vec3(2., 0., -2.), vec3(0.4, 0.4, 0.4), None, BLACK);

        draw_sphere(vec3(-8., 0., 0.), 1., None, BLUE);

        // Back to screen space, render some text

        set_default_camera();
        draw_text("r_flightsim7", 10.0, 20.0, 30.0, BLACK);

        next_frame().await
    }
}: Obj