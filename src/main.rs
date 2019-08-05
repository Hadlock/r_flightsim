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
// screen constants
static SCREEN_WIDTH: f64 = 600.0;
static SCREEN_HEIGHT:  f64 = 480.0;

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
    x: f64,
    y: f64,
    z: f64,
}

impl Position {
    fn up(&mut self) {
        self.y += 1.0;
    }
    fn position(&mut self, a: f64, b: f64, c: f64) {
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

// camera position
//static mut camPosition : Position(x = 0.0, y = -2.0 , z = 0.0);

const CAMPOSITION: Position = Position {
    x: 0.0,
    y: -2.0,
    z: 0.0,
};

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

struct Cube {
    wires: [u8; 12],
}

impl Default for Cube {
    fn default() -> Cube {
        Cube {
            wires: [0; 12],
        }
    }
    /*
    fn cube() -> Cube {
        Cube{
            wires[0] = Wire
        }
    }
    */
}

// Projects point onto camera "canvas"
fn point_on_canvas(pos: Position) -> Position {
    let mut angle_h = pos.y.atan2(pos.x) as f64;
    let mut angle_v = pos.z.atan2(pos.x) as f64;

    angle_h /= (angle_h.cos()).abs();
    angle_v /= (angle_v.cos()).abs();

    return Position { x: (SCREEN_WIDTH / 2.0 - angle_h * SCREEN_WIDTH / FOV) ,
                      y: (SCREEN_HEIGHT/2.0 - angle_v * SCREEN_WIDTH / FOV) , 
                      z: 0.0 }
}

fn to_cam_coords(pos: Position) -> Position{
  let r_pos = Position{x: 0.0, y: -2.0, z: 0.0};
  /*
  //calculating rotation
  float rx=rPos.x;
  float ry=rPos.y;
  float rz=rPos.z;
  
  //rotation z-axis
  rPos.x=rx*cos(-direction)-ry*sin(-direction);
  rPos.y=rx*sin(-direction)+ry*cos(-direction);
  
  //rotation y-axis
  rx=rPos.x;
  rz=rPos.z;
  rPos.x=rx*cos(-rotationY)+rz*sin(-rotationY);
  rPos.z=rz*cos(-rotationY)-rx*sin(-rotationY);
  */
  return r_pos;
}


fn window() {
    // camera direction
    let mut direction = 0;

    // cam position
    let mut cam_position = Position::default();

    // splice in piston window here
    let mut window: PistonWindow = 
        WindowSettings::new("r_flightsim", [SCREEN_WIDTH, SCREEN_HEIGHT])
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
                cam_position.x -= 1.0;
            }
            if key == Key::D {
                println!("right/stbd");
                hdg += 1;
                cam_position.x += 1.0;
            }
            // positional stuff
            println!("alt = {}, hdg = {}", alt, hdg);
            cam_position.position(8.0,7.0,2.0);
            println!("cam:: x = {}, y = {}, z = {}", cam_position.x, cam_position.y, cam_position.z);
        }


        window.draw_2d(&e, |c, g, _device| {
            clear([0.0; 4], g);

            // crosshairs
            line(game_colors::WHITE, 
                0.5, 
                    [
                        SCREEN_WIDTH/2.0-5.0 as f64,
                        SCREEN_HEIGHT/2.0 as f64,
                        SCREEN_WIDTH/2.0+5.0 as f64,
                        SCREEN_HEIGHT/2.0 as f64
                        ],
                c.transform, g);

            line(game_colors::WHITE, 
                0.5, 
                    [
                        SCREEN_WIDTH/2.0 as f64,
                        SCREEN_HEIGHT/2.0-5.0 as f64,
                        SCREEN_WIDTH/2.0 as f64,
                        SCREEN_HEIGHT/2.0+5 as f64
                        ],
                c.transform, g);

            // things that move
            line(game_colors::WHITE, 0.5, 
                    [
                        100.0 + hdg as f64, 
                        350.0 + alt as f64, 
                        300.0 + hdg as f64, 
                        350.0 + alt as f64
                        ], 
                        c.transform, 
                        g);
            /*
            // draw moving things:
            let c = cube.wires.length
            for i in 0..c {

                //wires end and start positions transformed to camera coordinates
                let camPosStart = toCamCoords(cube.wires[i].start);
                let camPosEnd = toCamCoords(cube.wires[i].end);
                
                //projection of start and endpoints to camera
                let drawStart = pointOnCanvas(camPosStart);
                let drawEnd = pointOnCanvas(camPosEnd);
                
                //drawing lines on screen
                line(game_colors::WHITE, 0.5, 
                    [
                        drawStart.x as f64, 
                        drawStart.y as f64, 
                        drawEnd.x as f64, 
                        drawEnd.y as f64
                        ], 
                        c.transform, 
                        g);
            }
            */

















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