// cli
extern crate clap;

// piston graphics and stuff
extern crate piston_window;
extern crate touch_visualizer;

// fps counter
// extern crate fps_counter;

// clap
use clap::{Arg, App};

// piston things
use piston_window::*;
use touch_visualizer::TouchVisualizer;

/// Contains colors that can be used in the game
pub mod game_colors {
    pub const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    pub const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    pub const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
    pub const LIGHTBLUE: [f32; 4] = [0.0, 1.0, 1.0, 1.0];
    pub const ORANGE: [f32; 4] = [1.0, 0.5, 0.0, 1.0];
    pub const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
    pub const PINK: [f32; 4] = [1.0, 0.0, 1.0, 1.0];
    pub const ANGEL: [f32; 4 ] = [0.5,0.5,1.0,0.5];
    pub const GREEN: [f32; 4 ] = [0.0,0.5,0.0,1.0];
}

fn window() {


    // splice in piston window here
    let mut window: PistonWindow = 
        WindowSettings::new("r_flightsim", [960, 700])
        .exit_on_esc(true).build().unwrap();

    // button handle boilerplate
    let mut touch_visualizer = TouchVisualizer::new();
    let _events = Events::new(EventSettings::new().lazy(true));

    let mut alt = 0;
    let mut hdg = 0;
    println!("alt = {}", alt);
    println!("hdg = {}", hdg);


    while let Some(e) = window.next() {
        // button handle loop
        touch_visualizer.event(window.size(), &e);
        
        if let Some(Button::Keyboard(key)) = e.press_args() {
            if key == Key::W {
                println!("down");
                alt += 1;
                println!("alt = {}", alt);
            }
            if key == Key::S {
                println!("up");
                alt -= 1;
                println!("alt = {}", alt);
            }
            if key == Key::A {
                println!("left/port");
                hdg -= 1;
                println!("hdg = {}", hdg);
            }
            if key == Key::D {
                println!("right/stbd");
                hdg += 1;
                println!("hdg = {}", hdg);
            }
            
        }


        window.draw_2d(&e, |c, g, _device| {
            clear([0.0; 4], g);
            line(game_colors::WHITE, 0.5, [0.0, 0.0, 200.0, 200.0], c.transform, g);
            line(game_colors::WHITE, 0.5, [0.0 + hdg as f64, 200.0 + alt as f64, 200.0 + hdg as f64, 200.0 + alt as f64], c.transform, g);
        });
    }
}


fn main() {
    
    // clap cli arguments
    let _matches = App::new("r_flightsim")
    .version("0.1")
    .author("Chad Hedstrom")
    .about("Flightsim written in rust")
    .arg(Arg::with_name("verbose")
        .short("v")
        .multiple(true)
        .help("verbosity level"))
    .get_matches();


    println!("---- r_flightsim Start ----");

    window();
}