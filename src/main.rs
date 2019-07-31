// cli
extern crate ctrlc;
extern crate clap;

// piston graphics and stuff
extern crate piston_window;
extern crate touch_visualizer;

// fps counter
// extern crate fps_counter;

//ctrlc
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// clap
use std::process;
use clap::{Arg, ArgMatches, App, SubCommand};

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
        WindowSettings::new("r_flightsim", [640, 480])
        .exit_on_esc(true).build().unwrap();

    // button handle boilerplate
    let mut touch_visualizer = TouchVisualizer::new();
    let mut events = Events::new(EventSettings::new().lazy(true));

    while let Some(e) = window.next() {
        // button handle loop
        touch_visualizer.event(window.size(), &e);

        if let Some(Button::Keyboard(key)) = e.press_args() {
            if key == Key::G {
                println!("Retracted landing gear");
            }
        }



        window.draw_2d(&e, |c, g, _device| {
            clear([0.0; 4], g);
            rectangle(game_colors::RED, // red
                      [100.0, 100.0, 100.0, 100.0],
                      c.transform, g);
            rectangle(game_colors::GREEN, // green
                      [200.0, 200.0, 100.0, 100.0],
                      c.transform, g);
            rectangle(game_colors::BLUE, // green
                      [300.0, 300.0, 100.0, 100.0],
                      c.transform, g);
            for i in 0..5 {
            line(game_colors::WHITE, 1.0, [320.0 + i as f64 * 15.0, 20.0, 380.0 - i as f64 * 15.0, 80.0],
                      c.transform, g);
            }
        });
    }
}


fn main() {
    
    // clap cli arguments
    let matches = App::new("r_flightsim")
    .version("0.1")
    .author("Chad Hedstrom")
    .about("Flightsim written in rust")
    .arg(Arg::with_name("verbose")
        .short("v")
        .multiple(true)
        .help("verbosity level"))
    .get_matches();

    // ctrl c stuff
    /*
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");
    println!("Waiting for Ctrl-C...");
    */
    println!("---- r_flightsim Start ----");

    window();

    /*
    while running.load(Ordering::SeqCst) {}
    println!("Got it! Exiting...");
    */
}