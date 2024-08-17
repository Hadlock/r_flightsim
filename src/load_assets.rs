use macroquad::prelude::*;
use tobj::Mesh;

pub struct Assets {
    pub rust_logo: Texture2D,
    pub ferris: Texture2D,
    pub vertices1: Vec<Vec3>,
    pub vertices2: Vec<Vec3>,
    pub mesh1: Mesh,
    pub mesh2: Mesh,
}

pub async fn load_assets() -> Assets {
    // Load textures
    let rust_logo = load_texture("src/rust.png").await.unwrap();
    let ferris = load_texture("src/ferris.png").await.unwrap();

    // Load OBJ models
    let (models1, _) = tobj::load_obj("src/obj/teapot.obj", &tobj::LoadOptions::default()).unwrap();
    let (models2, _) = tobj::load_obj("src/obj/skytrain400f.obj", &tobj::LoadOptions::default()).unwrap();

    let mesh1 = models1[0].mesh.clone();
    let mesh2 = models2[0].mesh.clone();

    // Convert vertices to Vec3
    let vertices1 = mesh1.positions.chunks(3).map(|v| vec3(v[0], v[1], v[2])).collect();
    let vertices2 = mesh2.positions.chunks(3).map(|v| vec3(v[0], v[1], v[2])).collect();

    Assets {
        rust_logo,
        ferris,
        vertices1,
        vertices2,
        mesh1,
        mesh2,
    }
}