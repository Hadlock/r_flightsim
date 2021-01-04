// r_flightsim a wireframe flight simulator

// region boilerplate
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

// annoying const

// camera position
//static mut camPosition : Position(x = 0.0, y = -2.0 , z = 0.0);
const CAMPOSITION: cube::Position = cube::Position {
  // this is probably in a bad spot
  x: 0.0,
  y: -2.0,
  z: 0.0,
};

// endregion

struct MainState {
  // circle pos
  pos_x: f32,
  pos_y: f32,

  // new cube
  newcube: cube::Cube,

  // camera state
  cam_pos: cube::Position,
  direction: f32,
  rotation_y: f32,
  prev_mouse_x: f32,
  prev_mouse_y: f32,

  // gui things
  imgui_wrapper: ImGuiWrapper,
  hidpi_factor: f32,
}

impl MainState {
    //fn new() -> GameResult<MainState> {
      fn new(mut ctx: &mut Context, hidpi_factor: f32) -> GameResult<MainState> {
        let imgui_wrapper = ImGuiWrapper::new(&mut ctx);
        let s = MainState {
            // circle position
            pos_x: 300.0,
            pos_y: 160.0,

            // new cube
            newcube: cube::prime_cube(),
  
            // camera
            cam_pos: CAMPOSITION, // wasd
            direction: std::f32::consts::FRAC_PI_8, // cam - mouse X
            rotation_y: 0.0, // cam - mouse Y
            prev_mouse_x: 0.0,
            prev_mouse_y: 0.0,
            
            // gui boilerplate
            imgui_wrapper,
            hidpi_factor,
        };
        Ok(s)
    }
}

impl EventHandler for MainState {

  // update game state

  fn update(&mut self, ctx: &mut Context) -> GameResult<()> {
      // region camera
      // circle movement... Increase or decrease `position_x` by 0.5, or by 5.0 if Shift is held.
      // D
      if keyboard::is_key_pressed(ctx, KeyCode::D) {
        if keyboard::is_mod_active(ctx, KeyMods::SHIFT) {
            self.pos_x += 4.5;
        }
        self.pos_x += 0.5;
      // A
      } else if keyboard::is_key_pressed(ctx, KeyCode::A) {
          if keyboard::is_mod_active(ctx, KeyMods::SHIFT) {
              self.pos_x -= 4.5;
          }
          self.pos_x -= 0.5;
      }
      // W
      if keyboard::is_key_pressed(ctx, KeyCode::W) {
        if keyboard::is_mod_active(ctx, KeyMods::SHIFT) {
            self.pos_y += 4.5;
        }
        self.pos_y += 0.5;
      // S
      } else if keyboard::is_key_pressed(ctx, KeyCode::S) {
          if keyboard::is_mod_active(ctx, KeyMods::SHIFT) {
              self.pos_y -= 4.5;
          }
          self.pos_y -= 0.5;
      }
      // endregion
      
      // region cube
      // CUBE position manipulation

      // L
      if keyboard::is_key_pressed(ctx, KeyCode::L) {
        if keyboard::is_mod_active(ctx, KeyMods::SHIFT) {
          self.newcube.cubepos.x += 4.5;
        }
        self.newcube.cubepos.x += 0.5;
      // J
      } else if keyboard::is_key_pressed(ctx, KeyCode::J) {
          if keyboard::is_mod_active(ctx, KeyMods::SHIFT) {
            self.newcube.cubepos.x -= 4.5;
          }
          self.newcube.cubepos.x -= 0.5;
      }
      // I
      if keyboard::is_key_pressed(ctx, KeyCode::I) {
        if keyboard::is_mod_active(ctx, KeyMods::SHIFT) {
          self.newcube.cubepos.y += 4.5;
        }
        self.newcube.cubepos.y += 0.5;
      // K
      } else if keyboard::is_key_pressed(ctx, KeyCode::K) {
          if keyboard::is_mod_active(ctx, KeyMods::SHIFT) {
            self.newcube.cubepos.y -= 4.5;
          }
          self.newcube.cubepos.y -= 0.5;
      }
      // endregion

      Ok(())
  }

  
  fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
    // draw new game state
    //graphics::clear(ctx, [0.1, 0.2, 0.3, 1.0].into());
    gui::graph(ctx);

    // begin engine draw
    
    // region crosshair draw
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
    // endregion

    // region cube draw
    {
      // newcube

      println!("mah cube is too large: {:?}", self.newcube.wires);
      println!("----------------------------------------");
      println!("supah kubeah: {:?}", self.newcube.cubepos);
      println!("");

      // ok lets draw a cube
      for i in 0..self.newcube.wires.len() {
        //wires end and start positions transformed to camera coordinates
        if consts::FIXEDCAM == 1 {
          // fixedcam
          fix_ggez_collisions(self.newcube.wires[i]); // actually neccessary

          let draw_start = point_on_canvas(self.newcube.wires[i].start);
          let draw_end = point_on_canvas(self.newcube.wires[i].end);
  
          // println!("Draw Start: {:?}", draw_start);
          // println!("Draw End: {:?}", draw_end);
  
          // draw a wire
          let (origin, dest) = (draw_start, draw_end);
          let line = graphics::Mesh::new_line(ctx, &[origin, dest], 1.0, graphics::WHITE)?;
          graphics::draw(ctx, &line, (na::Point2::new(0.0, 0.0),))?;          

        }
        else {
          // mousecam
          let cam_pos_start = new_2cam_coords(self.newcube.wires[i].start, self.cam_pos, self.rotation_y, self.direction);
          let cam_pos_end   = new_2cam_coords(self.newcube.wires[i].end, self.cam_pos, self.rotation_y, self.direction);
          
          fix_ggez_collisions(self.newcube.wires[i]); // actually neccessary
          
          let draw_start = point_on_canvas(cam_pos_start);
          let draw_end = point_on_canvas(cam_pos_end);
          
          // draw a wire
          let (origin, dest) = (draw_start, draw_end);
          let line = graphics::Mesh::new_line(ctx, &[origin, dest], 1.0, graphics::WHITE)?;
          graphics::draw(ctx, &line, (na::Point2::new(0.0, 0.0),))?;
          }
        }
      }
    // endregion

    // region circle draw
    {
    // render circle
    let circle = graphics::Mesh::new_circle(
      ctx,
      graphics::DrawMode::fill(),
      na::Point2::new(self.pos_x, self.pos_y),
      40.0,
      0.9,
      graphics::WHITE,
      )?;

      graphics::draw(ctx, &circle, graphics::DrawParam::default())?;
    }
    // endregion

    // end engine draw

    // draw gui
    {
      self.imgui_wrapper.render(ctx, self.hidpi_factor);
    }

    graphics::present(ctx)?;
    timer::yield_now();
    Ok(())


  }

  // region control events
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
      if consts::FIXEDCAM == 1 {
        // fixedcam
        self.direction = (self.prev_mouse_x-x)*2.0*consts::FOV/consts::SCREEN_WIDTH*4.0;
      }

      else {
        // mousecam
        //turning with the mouse

        // TODO: fix why cube stopped drawing pt 2 or 2
        self.direction = (self.prev_mouse_x-x)*2.0*consts::FOV/consts::SCREEN_WIDTH*4.0;
        while self.direction >= consts::PI2 {
          self.direction -= consts::PI2;
        }
        while self.direction < consts::PI2 {
          self.direction += consts::PI2;
        }

        self.rotation_y -= (self.prev_mouse_y-y)*2.0*consts::FOV/consts::SCREEN_HEIGHT;
        if self.rotation_y > consts::FOV {
          self.rotation_y = consts::FOV;
        }
        if self.rotation_y < (-consts::FOV) {
          self.rotation_y = -consts::FOV;
        }
        
      }


      // wrap up
      // set previous mouse X/Y for use later
      self.prev_mouse_x = x;
      self.prev_mouse_y = y;

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
// endregion

// region helper functions

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

pub fn to_cam_coords(_r_pos: cube::Position) -> cube::Position {
  let r_pos = cube::Position{x: 0.0, y: -2.0, z: 0.0};
  return r_pos;
}

pub fn new_2cam_coords( wire: cube::Position, 
                        cam_pos: cube::Position,
                        cam_rotation_y: f32,
                        cam_direction_x: f32) -> cube::Position {
  let mut r_pos = cube::Position{x: (wire.x-cam_pos.x),
                                 y: (wire.y-cam_pos.y),
                                 z: (wire.z-cam_pos.z)};
  
  //calculating rotation
  let mut rx = r_pos.x as f32;
  let     ry = r_pos.y as f32; // mut not needed!
  let     rz = r_pos.z as f32; // mut not needed!
  
  // rotation z-axis
  r_pos.x = rx*(-cam_direction_x.cos())-ry*(-cam_direction_x.sin());
  r_pos.y = rx*(-cam_direction_x.cos())+ry*(-cam_direction_x.cos());
  
  //rotation y-axis
  rx = r_pos.x;
  // rz = r_pos.z; no need to reassign this
  r_pos.x = rx*(-cam_rotation_y.cos())+rz*(-cam_rotation_y.sin());
  r_pos.z = rz*(-cam_rotation_y.cos())-rx*(-cam_rotation_y.sin());
  
  return r_pos;  
}

pub fn fix_ggez_collisions(mut wire: cube::Wire) -> cube::Wire {
  // ggez freaks out if the line has zero length
  // there is no way to override this behavior

  // this can (does) happen if (when) the perspective is just right
  if wire.start.x == wire.end.x {
    if wire.start.y == wire.end.y {
      // add graphically 0 length to wire position to get around
      // ggez literal value limitation
      wire.end.x = wire.end.x + 0.001;
    }
  }
  return wire;
}
// endregion

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