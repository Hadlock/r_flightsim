// these are all the critical parts of the code, probably

// Projects point onto camera "canvas"
pub fn point_on_canvas(pos: Position) -> Position {
    let mut angle_h = pos.y.atan2(pos.x) as f64;
    let mut angle_v = pos.z.atan2(pos.x) as f64;

    angle_h /= (angle_h.cos()).abs();
    angle_v /= (angle_v.cos()).abs();

    return Position { x: (SCREEN_WIDTH / 2.0 - angle_h * SCREEN_WIDTH / FOV) ,
                      y: (SCREEN_HEIGHT/2.0 - angle_v * SCREEN_WIDTH / FOV) ,
                      z: 0.0 }
}

pub fn to_cam_coords(pos: Position) -> Position{
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


pub fn important_code() {

    // instantiate cube
    let cube = Cube::default();

    for i in 0..cube.wires.len() {

        //wires end and start positions transformed to camera coordinates
        let cam_pos_start = to_cam_coords(cube.wires[i].start);
        let cam_pos_end = to_cam_coords(cube.wires[i].end);

        //projection of start and endpoints to camera
        let draw_start = point_on_canvas(cam_pos_start);
        let draw_end = point_on_canvas(cam_pos_end);

        //drawing lines on screen
        line(game_colors::WHITE, 0.5,
            [
                draw_start.x as f64,
                draw_start.y as f64,
                draw_end.x as f64,
                draw_end.y as f64
                ],
                c.transform,
                g);
    }
}