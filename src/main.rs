// cli
use clap;

// clap
use clap::{Arg, App};

// chads stuff
mod cube;
mod consts;


// ui crap
use crate::imgui_wrapper::ImGuiWrapper;
// chad's ui crap
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

// end annoying consts


struct MainState {
  pos_x: f32,
  pos_y: f32,
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
            imgui_wrapper,
            hidpi_factor,
        };
        Ok(s)
    }
}

impl EventHandler for MainState {

  // update game state

  fn update(&mut self, ctx: &mut Context) -> GameResult<()> {
      // Increase or decrease `position_x` by 0.5, or by 5.0 if Shift is held.
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

      Ok(())
  }

  // draw new game state

  fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
    //graphics::clear(ctx, [0.1, 0.2, 0.3, 1.0].into());
    gui::graph(ctx);

    // BEGIN ACTUAL DRAW ////////////////////



    // first attempt at a cube, delete me

    {
      /*
      let cube = cube::cube_funtimes();

      for i in 0..cube.wires.len() {

             // crosshair vertical line
      let (origin, dest) = (na::Point2::new(307.0, 207.0), na::Point2::new(309.0, 289.0));
      let line = graphics::Mesh::new_line(ctx, &[origin, dest], 1.0, graphics::WHITE)?;
      graphics::draw(ctx, &line, (na::Point2::new(0.0, 0.0),))?;
              }
      */
    }

    //
    
    {
      // attempt at crosshairs

      //  line(screenWidth/2-5, screenHeight/2, [screenWidth/2+5, screenHeight/2);
      //  line(screenWidth/2, screenHeight/2-5,[screenWidth/2, screenHeight/2+5);

      // consts::SCREEN_WIDTH, consts::SCREEN_HEIGHT

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

        /*

        // pretty ugly that I'm multipling by 10
        let startx = cube.wires[i].start.x * 10 as f32;
        let starty = cube.wires[i].start.y * 10 as f32;
        let mut endx = cube.wires[i].end.x * 10 as f32;
        let endy = cube.wires[i].end.y * 10 as f32;

        if startx == endx {
          // ggez freaks out if the line has zero length
          if starty == endy {
            println!("collison found");
            endx = endx + 0.1;
          }
        }
        
        println!("Start X: {:?}", startx);
        println!("Start Y: {:?}", starty);
        println!("End X: {:?}", endx);
        println!("End Y: {:?}", endy);

        let nextk = cube.wires[i].start; //(1.0, 1.0) as na::Point2<f32>
        let naz = pos_to_napt2(cube.wires[i].start);
        //println!("Next K: {:?}", nextk);
        //println!("Naz Z: {:?}", naz);

        */

        if cube.wires[i].start.x == cube.wires[i].end.x {
          // ggez freaks out if the line has zero length
          if cube.wires[i].start.y == cube.wires[i].end.y {
            println!("collison found");
            cube.wires[i].end.x = cube.wires[i].end.x + 0.001;
          }
        }


        let nas = pos_to_napt2(cube.wires[i].start);
        let nae = pos_to_napt2(cube.wires[i].end);

        println!("Nas S: {:?}", nas);
        println!("Nae E: {:?}", nae);


        // draw a wire
        //let (origin, dest) = (na::Point2::new(startx, cube.wires[i].start.y), na::Point2::new(cube.wires[i].end.x, cube.wires[i].end.y));      
        let (origin, dest) = (nas, nae);
        
        let line = graphics::Mesh::new_line(ctx, &[origin, dest], 1.0, graphics::WHITE)?;
        graphics::draw(ctx, &line, (na::Point2::new(0.0, 0.0),))?;
        }
      }

    
    //
    {
      //generic rectangle
      /*
      let rectangle = graphics::Mesh::new_rectangle(
        ctx,
        graphics::DrawMode::fill(),
        [0.0, 0.0, 30.0, 30.0].into(),
        graphics::WHITE,
        )?;
      graphics::draw(ctx, &rectangle, (na::Point2::new(0.0, 0.0),))?;
      */
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

    // END ACTUAL DRAW ////////////////////////

    // draw GUI things
    {
      self.imgui_wrapper.render(ctx, self.hidpi_factor);
    }

    graphics::present(ctx)?;
    timer::yield_now();
    Ok(())


  }

  // listen for control events

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
    // add mouse crap here

  fn mouse_motion_event(&mut self, _ctx: &mut Context, x: f32, y: f32, _dx: f32, _dy: f32) {
      self.imgui_wrapper.update_mouse_pos(x, y);
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

    // end mouse crap
    // end listen for control events
  }

// helper function garbage
  pub fn point_on_canvas(pos: cube::Position) -> cube::Position {
    let mut angle_h = pos.y.atan2(pos.x) as f32;
    let mut angle_v = pos.z.atan2(pos.x) as f32;

    angle_h /= (angle_h.cos()).abs();
    angle_v /= (angle_v.cos()).abs();

    return cube::Position { 
      
        x: (consts::SCREEN_WIDTH / 2.0 - angle_h * consts::SCREEN_WIDTH / consts::FOV) ,
        y: (consts::SCREEN_HEIGHT/2.0 - angle_v * consts::SCREEN_WIDTH / consts::FOV) ,
        z: 0.0 }
}

pub fn pos_to_napt2(pos: cube::Position) -> na::Point2<f32> {
  // ba ba ba ran
  let mut angle_h = pos.y.atan2(pos.x) as f32;
  let mut angle_v = pos.z.atan2(pos.x) as f32;

  angle_h /= (angle_h.cos()).abs();
  angle_v /= (angle_v.cos()).abs();

  let newx = (consts::SCREEN_WIDTH / 2.0 - angle_h * consts::SCREEN_WIDTH / consts::FOV) as f32;
  let newy = (consts::SCREEN_HEIGHT/2.0 - angle_v * consts::SCREEN_WIDTH / consts::FOV) as f32;
  return na::Point2::new(newx, newy)
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


    println!("---- r_flightsim Start ----");

    // just chad things

    // cube::cube_funtimes();

    // gui boilerplate

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