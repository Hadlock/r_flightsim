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

// camera FOV
static FOV: f64 = std::f64::consts::FRAC_PI_2;

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

// camera position
#[derive(Debug, Default)]
struct Position {
    x: i32,
    y: i32,
    z: i32,
}

impl Position {
    fn up(&mut self) {
        self.y += 1;
    }
    fn position(&mut self, a: i32, b: i32, c: i32) {
        self.x = a;
        self.y = b;
        self.z = c;
    }

    fn relative(c: Position, d: Position) -> Position {
        // finds relative position between two points
        // returns new position struct
        return Position { x:  c.x + d.x, y: c.y + d.y, z: c.z + d.z }
    }
}

struct Wire {
    start: Position,
    end: Position,
}

impl Wire {
    fn wire(&mut self, s: Position, e: Position) {
        self.start = s;
        self.end = e;
    }

}

fn window() {
    // screen constants
    let screen_width=600 as f64;
    let screen_height=480 as f64;
    
    // camera direction
    let mut direction = 0;

    // cam position
    let mut cam_position = Position::default();

    // splice in piston window here
    let mut window: PistonWindow = 
        WindowSettings::new("r_flightsim", [screen_width, screen_height])
        .exit_on_esc(true).build().unwrap();

    // button handle boilerplate
    let mut touch_visualizer = TouchVisualizer::new();
    let _events = Events::new(EventSettings::new().lazy(true));

    let mut alt = 0;
    let mut hdg = 0;

    while let Some(e) = window.next() {
        // button handle loop
        touch_visualizer.event(window.size(), &e);
        
        if let Some(Button::Keyboard(key)) = e.press_args() {
            if key == Key::W {
                println!("down");
                alt += 1;
            }
            if key == Key::S {
                println!("up");
                alt -= 1;
            }
            if key == Key::A {
                println!("left/port");
                hdg -= 1;
                cam_position.x -= 1;
            }
            if key == Key::D {
                println!("right/stbd");
                hdg += 1;
                cam_position.x += 1;
            }
            // positional stuff
            println!("alt = {}, hdg = {}", alt, hdg);
            cam_position.position(8,7,2);
            println!("cam:: x = {}, y = {}, z = {}", cam_position.x, cam_position.y, cam_position.z);
        }


        window.draw_2d(&e, |c, g, _device| {
            clear([0.0; 4], g);

            // crosshairs
            line(game_colors::WHITE, 
                0.5, 
                    [
                        screen_width/2.0-5.0 as f64,
                        screen_height/2.0 as f64,
                        screen_width/2.0+5.0 as f64,
                        screen_height/2.0 as f64
                        ],
                c.transform, g);

            line(game_colors::WHITE, 
                0.5, 
                    [
                        screen_width/2.0 as f64,
                        screen_height/2.0-5.0 as f64,
                        screen_width/2.0 as f64,
                        screen_height/2.0+5 as f64
                        ],
                c.transform, g);

            // things that move
            line(game_colors::WHITE, 0.5, [100.0 + hdg as f64, 350.0 + alt as f64, 300.0 + hdg as f64, 350.0 + alt as f64], c.transform, g);
        });
    }
}


fn main() {
    
    // clap cli arguments
    let _matches = App::new("r_flightsim")
    .version("0.1.0")
    .author("Chad Hedstrom")
    .about("Flightsim written in rust")
    .arg(Arg::with_name("verbose")
        .short("v")
        .multiple(true)
        .help("verbosity level"))
    .get_matches();


    println!("---- r_flightsim Start ----");
    println!("This is fov {}", FOV);
    window();
}