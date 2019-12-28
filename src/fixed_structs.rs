#![allow(dead_code)]

static SCREEN_WIDTH: f64 = 600.0;
static SCREEN_HEIGHT:  f64 = 480.0;

static DIRECTION: f64 = 0.392; // probably not a static, long term
static ROTATIONY: f64 = 0.0;   // also probably not a static, long term

static FOV: f64 = std::f64::consts::FRAC_PI_2;

static SPEED: f64 = 0.1;

#[derive(Debug, Default, Copy, Clone)]
struct Position {
    x: f64,
    y: f64,
    z: f64,
}

impl Position {
  fn default1(&mut self) {
    self.x = 0.0;
    self.y = 0.0;
    self.z = 0.0;
  }

  fn relative(&mut self, rx: f64, ry: f64, rz: f64) -> Position {
      let a = self.x + rx;
      let b = self.y + ry;
      let c = self.z + rz;
      let relpos = Position{x: a, y: b, z: c};
      relpos
    }

}

#[derive(Debug, Default, Copy, Clone)]
struct Wire {
  start: Position,
  end: Position,
}

#[derive(Debug, Default, Copy, Clone)]
struct Cube {
  wires: [Wire; 12],
}

impl Cube {
  fn cube(&mut self, w: [Wire; 12]) {
    self.wires = w;
  }
}

fn wire_funtimes() -> Wire {
    let mut mypos = Position{x: 7.0, y: 8.0, z: 9.0};
    let myotherpos = Position{x: 88.0, y: 77.0, z: 66.0};
    mypos = mypos.relative(99.0, 99.0, 99.0);
    println!("{:?}", mypos);
    
    let mywire = Wire{start: mypos, end: myotherpos};
    println!("{:?}", mywire);
    mywire
}

fn cube_funtimes() -> Cube {
    //let zenwire = Wire{..wire_funtimes()};

    let mut cubepos = Position{x: 6.0, y: 0.0, z: -2.0};
    let size: f64 = 2.0;

    let cool_cube = Cube{
        wires:
        [
            //   wires[0] = new Wire(pos.relative(size/2, size/2, size/2), pos.relative(-size/2, size/2, size/2));
            Wire{ start: cubepos.relative(size/2.0, size/2.0, size/2.0), end: cubepos.relative(-size/2.0, size/2.0, size/2.0) },
            //wires[1] = new Wire(pos.relative(size/2, -size/2, size/2), pos.relative(-size/2, -size/2, size/2));
            Wire{ start: cubepos.relative(size/2.0, -size/2.0, size/2.0), end: cubepos.relative(-size/2.0, -size/2.0, size/2.0) },
            //wires[2] = new Wire(pos.relative(size/2, size/2, size/2), pos.relative(size/2, -size/2, size/2));
            Wire{ start: cubepos.relative(size/2.0, size/2.0, size/2.0), end: cubepos.relative(size/2.0, -size/2.0, size/2.0) },
            //wires[3] = new Wire(pos.relative(-size/2, size/2, size/2), pos.relative(-size/2, -size/2, size/2));
            Wire{ start: cubepos.relative(-size/2.0, size/2.0, size/2.0), end: cubepos.relative(-size/2.0, -size/2.0, size/2.0) },
            //wires[4] = new Wire(pos.relative(size/2, size/2, -size/2), pos.relative(-size/2, size/2, -size/2));
            Wire{ start: cubepos.relative(size/2.0, size/2.0, -size/2.0), end: cubepos.relative(-size/2.0, size/2.0, -size/2.0) },
            //wires[5] = new Wire(pos.relative(size/2, -size/2, -size/2), pos.relative(-size/2, -size/2, -size/2));
            Wire{ start: cubepos.relative(size/2.0, -size/2.0, -size/2.0), end: cubepos.relative(-size/2.0, -size/2.0, -size/2.0) },
            //wires[6] = new Wire(pos.relative(size/2, size/2, -size/2), pos.relative(size/2, -size/2, -size/2));
            Wire{ start: cubepos.relative(size/2.0, size/2.0, -size/2.0), end: cubepos.relative(size/2.0, -size/2.0, -size/2.0) },
            //wires[7] = new Wire(pos.relative(-size/2, size/2, -size/2), pos.relative(-size/2, -size/2, -size/2));  
            Wire{ start: cubepos.relative(-size/2.0, size/2.0, -size/2.0), end: cubepos.relative(-size/2.0, -size/2.0, -size/2.0) },
            //wires[8] = new Wire(pos.relative(size/2, size/2, size/2), pos.relative(size/2, size/2, -size/2));
            Wire{ start: cubepos.relative(size/2.0, size/2.0, size/2.0), end: cubepos.relative(size/2.0, size/2.0, -size/2.0) },
            //wires[9] = new Wire(pos.relative(size/2, -size/2, size/2), pos.relative(size/2, -size/2, -size/2));
            Wire{ start: cubepos.relative(size/2.0, -size/2.0, size/2.0), end: cubepos.relative(size/2.0, -size/2.0, -size/2.0) },
            //wires[10] = new Wire(pos.relative(-size/2, -size/2, size/2), pos.relative(-size/2, -size/2, -size/2));
            Wire{ start: cubepos.relative(-size/2.0, -size/2.0, size/2.0), end: cubepos.relative(-size/2.0, -size/2.0, -size/2.0) },
            //wires[11] = new Wire(pos.relative(-size/2, size/2, size/2), pos.relative(-size/2, size/2, -size/2));
            Wire{ start: cubepos.relative(-size/2.0, size/2.0, size/2.0), end: cubepos.relative(-size/2.0, size/2.0, -size/2.0) },
        ]
    };
    println!("PRETTY COOL CUBE: {:?}", cool_cube);
    cool_cube
}

fn main() {
    // boilerplate drama llama

    // wire_funtimes();
    cube_funtimes();
}