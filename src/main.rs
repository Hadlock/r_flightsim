// cli
use clap;

// clap
use clap::{Arg, App};

// chads stuff
mod cube;
mod consts;
mod logo;


// ui crap
use crate::imgui_wrapper::ImGuiWrapper;
mod gui;

mod imgui_wrapper;

// boilerplate
//use ggez::conf;
use ggez::event::{self, EventHandler, KeyCode, KeyMods, MouseButton};
use ggez::{graphics, nalgebra as na, timer};
use ggez::input::keyboard;
use ggez::{Context, GameResult};

use ggez::conf::{WindowMode, WindowSetup};

// annoying consts

// camera position
//static mut camPosition : Position(x = 0.0, y = -2.0 , z = 0.0);
const CAMPOSITION: cube::Position = cube::Position {
  // this is probably in a bad spot
  x: 0.0,
  y: -2.0,
  z: 0.0,
};

struct MainState {
  // circle pos
  pos_x: f32,
  pos_y: f32,

  // camera state
  direction: f32,
  rotation_y: f32,
  pmousex: f32,
  pmousey: f32,
  
  // cube pos
    //  cub_x: f32,
    //  cub_y: f32,

  // gui things
  imgui_wrapper: ImGuiWrapper,
  hidpi_factor: f32,
}

impl MainState {
    //fn new() -> GameResult<MainState> {
      fn new(mut ctx: &mut Context, hidpi_factor: f32) -> GameResult<MainState> {
        let imgui_wrapper = ImGuiWrapper::new(&mut ctx);
        let s = MainState {
            pos_x: 200.0,
            pos_y: 200.0,
            direction: std::f32::consts::FRAC_PI_8, // PI/8,
            rotation_y: 0.0,
            pmousex: 0.0,
            pmousey: 0.0,
            imgui_wrapper,
            hidpi_factor,
        };
        Ok(s)
    }
}

impl EventHandler for MainState {

  // update game state

  fn update(&mut self, ctx: &mut Context) -> GameResult<()> {
      // circle movement... Increase or decrease `position_x` by 0.5, or by 5.0 if Shift is held.
      if keyboard::is_key_pressed(ctx, KeyCode::D) {
        if keyboard::is_mod_active(ctx, KeyMods::SHIFT) {
            self.pos_x += 4.5;
        }
        self.pos_x += 0.5;
      } else if keyboard::is_key_pressed(ctx, KeyCode::A) {
          if keyboard::is_mod_active(ctx, KeyMods::SHIFT) {
              self.pos_x -= 4.5;
          }
          self.pos_x -= 0.5;
      }
      if keyboard::is_key_pressed(ctx, KeyCode::W) {
        if keyboard::is_mod_active(ctx, KeyMods::SHIFT) {
            self.pos_y += 4.5;
        }
        self.pos_y += 0.5;
      } else if keyboard::is_key_pressed(ctx, KeyCode::S) {
          if keyboard::is_mod_active(ctx, KeyMods::SHIFT) {
              self.pos_y -= 4.5;
          }
          self.pos_y -= 0.5;
      }

      // cube movement




      Ok(())
  }

  
  fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
    // draw new game state
    //graphics::clear(ctx, [0.1, 0.2, 0.3, 1.0].into());
    gui::graph(ctx);

    // begin engine draw
    
    {
      // crosshairs

      // horizontal
      let (origin, dest) = (na::Point2::new(consts::SCREEN_WIDTH/2.0-5.0, consts::SCREEN_HEIGHT/2.0), na::Point2::new(consts::SCREEN_WIDTH/2.0+5.0, consts::SCREEN_HEIGHT/2.0));
      let line = graphics::Mesh::new_line(ctx, &[origin, dest], 1.0, graphics::WHITE)?;
      graphics::draw(ctx, &line, (na::Point2::new(0.0, 0.0),))?;
      // vertical
      let (origin, dest) = (na::Point2::new(consts::SCREEN_WIDTH/2.0, consts::SCREEN_HEIGHT/2.0-5.0), na::Point2::new(consts::SCREEN_WIDTH/2.0, consts::SCREEN_HEIGHT/2.0+5.0));
      let line = graphics::Mesh::new_line(ctx, &[origin, dest], 1.0, graphics::WHITE)?;
      graphics::draw(ctx, &line, (na::Point2::new(0.0, 0.0),))?;

    }
    
    // ok lets draw a cube
    {
      let mut cube = cube::cube_funtimes();

      for i in 0..cube.wires.len() {

        
        //wires end and start positions transformed to camera coordinates
        let cam_pos_start = to_cam_coords(cube.wires[i].start);
        /*
        let cam_pos_end = to_cam_coords(cube.wires[i].end);
        */

        if cube.wires[i].start.x == cube.wires[i].end.x {
          // ggez freaks out if the line has zero length
          if cube.wires[i].start.y == cube.wires[i].end.y {
            println!("collison found");
            cube.wires[i].end.x = cube.wires[i].end.x + 0.001;
          }
        }

        let draw_start = point_on_canvas(cube.wires[i].start);
        let draw_end = point_on_canvas(cube.wires[i].end);

        println!("Draw Start: {:?}", draw_start);
        println!("Draw End: {:?}", draw_end);


        // draw a cube wire
        let (origin, dest) = (draw_start, draw_end);
        let line = graphics::Mesh::new_line(ctx, &[origin, dest], 1.0, graphics::WHITE)?;
        graphics::draw(ctx, &line, (na::Point2::new(0.0, 0.0),))?;
        }
      }

    // Create a circle at `position_x` and draw

    {
    // render game stuff
    let circle = graphics::Mesh::new_circle(
      ctx,
      graphics::DrawMode::fill(),
      na::Point2::new(self.pos_x, self.pos_y),
      70.0,
      0.9,
      graphics::WHITE,
      )?;

      graphics::draw(ctx, &circle, graphics::DrawParam::default())?;
    }

    // end engine draw

    // draw gui
    {
      self.imgui_wrapper.render(ctx, self.hidpi_factor);
    }

    graphics::present(ctx)?;
    timer::yield_now();
    Ok(())


  }

  // listen for control events

  // process keyboard events
  fn key_down_event(
    &mut self,
      ctx: &mut Context,
      key: KeyCode,
      mods: KeyMods,
      _: bool) {

        match key {
          // Quit if Shift+Ctrl+Q is pressed.
          KeyCode::Q => {
              if mods.contains(KeyMods::SHIFT & KeyMods::CTRL) {
                  println!("Terminating!");
                  event::quit(ctx);
              } else if mods.contains(KeyMods::SHIFT) || mods.contains(KeyMods::CTRL) {
                  println!("You need to hold both Shift and Control to quit.");
              } else {
                  println!("Now you're not even trying!");
              }
          }
          _ => (),
        }
      match key {
        KeyCode::P => {
            self.imgui_wrapper.open_popup();
        }
        _ => (),
      }
  }
  
  // process mouse events
  fn mouse_motion_event(&mut self, _ctx: &mut Context, x: f32, y: f32, _dx: f32, _dy: f32) {
      self.imgui_wrapper.update_mouse_pos(x, y);

      // calculate direction for wireframe

      //direction+=(pmouseX-mouseX)*2*fov/screenWidth*4;
      self.direction = (self.pmousex-x)*2.0*consts::FOV/consts::SCREEN_WIDTH*4.0;
      
      {
        // this probably needs to go into the draw section
        let mut dir = self.direction;

        //while(direction>=2*PI) direction-=2*PI;
        //while(direction<2*PI) direction+=2*PI;
        while self.direction >= consts::PI2 {self.direction = dir-consts::PI2};
        // next line is broken TODO: fixme
        // while self.direction <= consts::PI2 {self.direction = dir+consts::PI2};
        }

      // wrap up
      // set previous mouse X/Y for use later
      self.pmousex = x;
      self.pmousey = y;

  }

  fn mouse_button_down_event(
      &mut self,
      _ctx: &mut Context,
      button: MouseButton,
      _x: f32,
      _y: f32,
      ) {
      self.imgui_wrapper.update_mouse_down((
          button == MouseButton::Left,
          button == MouseButton::Right,
          button == MouseButton::Middle,
      ));
  }

  fn mouse_button_up_event(
      &mut self,
      _ctx: &mut Context,
      _button: MouseButton,
      _x: f32,
      _y: f32,
      ) {
      self.imgui_wrapper.update_mouse_down((false, false, false));
  }

  }
// end listen for control events

// helper functions

pub fn point_on_canvas(pos: cube::Position) -> na::Point2<f32> {
  // this takes a 3D position and maps it to a location
  // on the canvas in 2D space
  // this takes a r_flightsim position and returns
  // a na::Point2 object that ggez can easily ingest

  let mut angle_h = pos.y.atan2(pos.x) as f32;
  let mut angle_v = pos.z.atan2(pos.x) as f32;

  angle_h /= (angle_h.cos()).abs();
  angle_v /= (angle_v.cos()).abs();

  let newx = (consts::SCREEN_WIDTH / 2.0 - angle_h * consts::SCREEN_WIDTH / consts::FOV) as f32;
  let newy = (consts::SCREEN_HEIGHT/2.0 - angle_v * consts::SCREEN_WIDTH / consts::FOV) as f32;
  return na::Point2::new(newx, newy)
}

pub fn to_cam_coords(pos: cube::Position) -> cube::Position {
  let r_pos = cube::Position{x: 0.0, y: -2.0, z: 0.0};

  // TODO: lines 286-299 need to be refactored as rust to work

  //calculating rotation
  let rx = r_pos.x as f32;
  let ry = r_pos.y as f32;
  let rz = r_pos.z as f32;


  
  //rotation z-axis
  //r_pos.x=rx*cos(-direction)-ry*sin(-direction);
  /*
  rPos.y=rx*sin(-direction)+ry*cos(-direction);

  //rotation y-axis
  rx=rPos.x;
  rz=rPos.z;
  rPos.x=rx*cos(-rotationY)+rz*sin(-rotationY);
  rPos.z=rz*cos(-rotationY)-rx*sin(-rotationY);
  */
  return r_pos;
}

pub fn main() -> GameResult {

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
    logo::logo();


    // gui config
    let hidpi_factor: f32;
    {
        // Create a dummy window so we can get monitor scaling information
        let cb = ggez::ContextBuilder::new("", "");
        let (_ctx, events_loop) = &mut cb.build()?;
        hidpi_factor = events_loop.get_primary_monitor().get_hidpi_factor() as f32;
        println!("main hidpi_factor = {}", hidpi_factor);
    }


    let cb = ggez::ContextBuilder::new("super_simple", "ggez")
      .window_setup(WindowSetup::default().title("r_flightsim"))
      .window_mode(
        WindowMode::default()
            .dimensions(consts::SCREEN_WIDTH, consts::SCREEN_HEIGHT)
            .resizable(false),
      );
    let (ctx, event_loop) = &mut cb.build()?;
    let state = &mut MainState::new(ctx, hidpi_factor)?;

    // ignition
    event::run(ctx, event_loop, state)

}