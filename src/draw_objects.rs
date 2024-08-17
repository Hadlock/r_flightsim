use macroquad::prelude::*;

pub async fn draw_objects(rust_logo: &Texture2D, ferris: &Texture2D, plane_position: Vec3) {
    draw_cube_wires(vec3(0., 1., -6.), vec3(2., 2., 2.), DARKGREEN);
    draw_cube_wires(vec3(0., 1., 6.), vec3(2., 2., 2.), DARKBLUE);
    draw_cube_wires(vec3(2., 1., 2.), vec3(2., 2., 2.), YELLOW);

    draw_plane(vec3(-8., 0., -8.), vec2(5., 5.), Some(ferris), WHITE);

    draw_cube(
        vec3(-5., 1., -2.),
        vec3(2., 2., 2.),
        Some(rust_logo),
        WHITE,
    );
    draw_cube(vec3(-5., 1., 2.), vec3(2., 2., 2.), Some(ferris), WHITE);


    fn draw_airplane(plane_position: Vec3, color: Color) {
        draw_cube_wires(plane_position, vec3(1., 1., 1.), color); //position, size

    }
    draw_airplane(plane_position, ORANGE);
}