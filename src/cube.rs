#[derive(Debug, Default, Copy, Clone)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Position {

  pub fn relative(&mut self, rx: f32, ry: f32, rz: f32) -> Position {
      let a = self.x + rx;
      let b = self.y + ry;
      let c = self.z + rz;
      let relpos = Position{x: a, y: b, z: c};
      relpos
    }

}

#[derive(Debug, Default, Copy, Clone)]
pub struct Wire {
  pub start: Position,
  pub end: Position,
}

#[derive(Debug, Default, Copy, Clone)]
pub struct Cube {
  pub wires: [Wire; 12],
  pub cubepos: Position,
}

impl Cube {
  //fn cube(&mut self, w: [Wire; 12]) {
  //  self.wires = w;
  //}
}

pub fn prime_cube() -> Cube {
  //let zenwire = Wire{..wire_funtimes()};

  let mut cubepos = Position{x: 6.0, y: 0.0, z: -1.5};
  let size: f32 = 2.0;
  //let Wire = cube::Wire;

  let cubecumber = Cube{
      cubepos: cubepos,
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
  // println!("");
  // println!("________PRIME CUBE: {:?}", cubecumber);
  cubecumber
}