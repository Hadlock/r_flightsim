use macroquad::prelude::*;
use tobj::Mesh;

pub struct Assets {
    pub rust_logo: Texture2D,
    pub ferris: Texture2D,
    pub vertices1: Vec<Vec3>,
    pub vertices2: Vec<Vec3>,
    pub mesh1: Mesh,
    pub mesh2: Mesh,
    pub bbox1: BoundingBox,
    pub bbox2: BoundingBox,
}

pub async fn load_assets() -> Assets {
    // Load textures
    let rust_logo = load_texture("src/texture/rust.png").await.unwrap();
    let ferris = load_texture("src/texture/ferris.png").await.unwrap();

    // Load OBJ models
    let (models1, _) = tobj::load_obj("src/obj/teapot.obj", &tobj::LoadOptions::default()).unwrap();
    let (models2, _) = tobj::load_obj("src/obj/skytrain400f.obj", &tobj::LoadOptions::default()).unwrap();

    let mesh1 = models1[0].mesh.clone();
    let mesh2 = models2[0].mesh.clone();

    // Convert vertices to Vec3
    let vertices1 = mesh1.positions.chunks(3).map(|v| vec3(v[0], v[1], v[2])).collect();
    let vertices2 = mesh2.positions.chunks(3).map(|v| vec3(v[0], v[1], v[2])).collect();

    // Calculate bounding boxes
    let bbox1 = calculate_aabb(&mesh1);
    let bbox2 = calculate_aabb(&mesh2);

    Assets {
        rust_logo,
        ferris,
        vertices1,
        vertices2,
        mesh1,
        mesh2,
        bbox1,
        bbox2,
    }
}

pub struct BoundingBox {
    pub min: Vec3,
    pub max: Vec3,
}

pub fn calculate_aabb(mesh: &Mesh) -> BoundingBox {
    let mut min = vec3(f32::MAX, f32::MAX, f32::MAX);
    let mut max = vec3(f32::MIN, f32::MIN, f32::MIN);

    for chunk in mesh.positions.chunks(3) {
        let vertex = vec3(chunk[0], chunk[1], chunk[2]);
        min = vec3(min.x.min(vertex.x), min.y.min(vertex.y), min.z.min(vertex.z));
        max = vec3(max.x.max(vertex.x), max.y.max(vertex.y), max.z.max(vertex.z));
    }

    BoundingBox { min, max }
}

pub fn check_collision(a: &BoundingBox, b: &BoundingBox) -> bool {
    // probably need to use the scaled values

    // not currently working
    (a.min.x <= b.max.x && a.max.x >= b.min.x) &&
    (a.min.y <= b.max.y && a.max.y >= b.min.y) &&
    (a.min.z <= b.max.z && a.max.z >= b.min.z)
}