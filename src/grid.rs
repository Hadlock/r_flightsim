use macroquad::prelude::*;

pub fn draw_grid_based_on_position(position: f32) {
    let mut gridspacing = 1.0;

    if position < 5.0 {
        gridspacing = 1.0;
    } else if position > 200.0 {
        gridspacing = 350.0;
    } else if position > 70.0 {
        gridspacing = 100.0;
    } else if position > 30.0 {
        gridspacing = 50.0;
    } else if position > 10.0 {
        gridspacing = 20.0;
    } else if position > 5.0 {
        gridspacing = 10.0;
    }

    draw_grid(100, gridspacing, GRAY, WHITE);
}