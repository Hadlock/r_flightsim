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
    fn relative(&mut self, dx: i32, dy: i32, dz: i32) {
        let k = self.x + dx;
        let l = self.y + dy;
        let m = self.z + dz;
        return Position::position(self, k,l,m);
    }
    fn melative(&mut self, a: i32, b: i32, c: i32) {
        self.x += a;
        self.y += b;
        self.z += c;
    }
}

fn main() {
    let mut cam_position = Position::default();
    cam_position.up();
    cam_position.position(8,7,2);
    let mut myrelative = Position::default();
    myrelative.position(8,7,2);
    myrelative.relative(2,3,8);
    println!("{:?}", cam_position);
    println!("{:?}", myrelative);
    let mut newtwo = Position::default();
    println!("{:?}", newtwo);
    let mut newone = Position::relative(&mut cam_position, 1,1,1);
    let mut newtwo = Position::melative(&mut cam_position, 1,1,1);
    println!("{:?}", newone);
    println!("{:?}", newtwo);
    println!("{:?}", cam_position);
}