use glam::{DVec3, Mat4, Quat};
use std::path::Path;
use wgpu::util::DeviceExt;

use crate::obj_loader::{self, MeshData};

pub struct SceneObject {
    pub name: String,
    pub vertex_buf: wgpu::Buffer,
    pub index_buf: wgpu::Buffer,
    pub index_count: u32,
    pub world_pos: DVec3,
    pub rotation: Quat,
    pub scale: f32,
    pub object_id: u32,
    pub edges_enabled: bool,
}

impl SceneObject {
    pub fn model_matrix_relative_to(&self, camera_pos: DVec3) -> Mat4 {
        let rel = self.world_pos - camera_pos;
        let translation = Mat4::from_translation(rel.as_vec3());
        let rotation = Mat4::from_quat(self.rotation);
        let scale = Mat4::from_scale(glam::Vec3::splat(self.scale));
        translation * rotation * scale
    }
}

struct MeshBuffers {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
}

fn upload_mesh(device: &wgpu::Device, mesh: &MeshData, label: &str) -> MeshBuffers {
    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("{} Vertex Buffer", label)),
        contents: bytemuck::cast_slice(&mesh.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("{} Index Buffer", label)),
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    MeshBuffers {
        vertex_buf,
        index_buf,
        index_count: mesh.indices.len() as u32,
    }
}

fn spawn(
    device: &wgpu::Device,
    mesh: &MeshData,
    name: &str,
    pos: DVec3,
    rotation: Quat,
    scale: f32,
    object_id: u32,
) -> SceneObject {
    let bufs = upload_mesh(device, mesh, name);
    SceneObject {
        name: name.to_string(),
        vertex_buf: bufs.vertex_buf,
        index_buf: bufs.index_buf,
        index_count: bufs.index_count,
        world_pos: pos,
        rotation,
        scale,
        object_id,
        edges_enabled: true,
    }
}

pub fn load_scene(device: &wgpu::Device) -> Vec<SceneObject> {
    let teapot_mesh = obj_loader::load_obj(Path::new("assets/teapot.obj"));
    let plane_mesh = obj_loader::load_obj(Path::new(
        "assets/14082_WWII_Plane_Japan_Kawasaki_Ki-61_v1_L2.obj",
    ));

    // Teapot OBJ: X=spout-to-handle(6.43), Y=height(3.15, up), Z=front-back(4.0)
    // Already Y-up, no rotation needed.
    // Real teapot: 0.35m long → scale = 0.35 / 6.434
    let teapot_scale = 0.35 / 6.434;

    // Plane OBJ: X=nose-to-tail(1.59), Y=wingspan(2.20), Z=height(0.62)
    // Need: X→World Z (north), Y→World X (east), Z→World Y (up)
    // This is a -120° rotation around the (1,1,1) axis.
    let plane_rot = Quat::from_axis_angle(
        glam::Vec3::new(1.0, 1.0, 1.0).normalize(),
        -2.0 * std::f32::consts::FRAC_PI_3,
    );
    // Real Ki-61: 8.94m long, 12m wingspan → scale from wingspan: 12.0 / 2.2019
    let plane_scale = 12.0 / 2.2019;

    let mut objects = Vec::new();
    let mut id = 1u32;

    // 10 teapots in a grid on Y=0, spaced ~1m apart
    let teapot_positions: [(f64, f64, f64); 1] = [
        (0.0, 0.0, 0.0),

    ];

    for (i, &(x, y, z)) in teapot_positions.iter().enumerate() {
        objects.push(spawn(
            device,
            &teapot_mesh,
            &format!("teapot_{}", i),
            DVec3::new(x, y, z),
            Quat::IDENTITY,
            teapot_scale as f32,
            id,
        ));
        id += 1;
    }

    // 10 planes on Y=0, spaced ~20m apart
    let plane_positions: [(f64, f64, f64); 1] = [
        (0.0, 0.0, 0.0),
    ];

    for (i, &(x, y, z)) in plane_positions.iter().enumerate() {
        objects.push(spawn(
            device,
            &plane_mesh,
            &format!("plane_{}", i),
            DVec3::new(x, y, z),
            plane_rot,
            plane_scale as f32,
            id,
        ));
        id += 1;
    }

    /*
    // Reference cubes: 1m, 10m, 30m — already real-world scale, no rotation.
    // Placed along Z axis, 10m edge-to-edge gaps, sitting on Y=0.
    let cube_1m = obj_loader::load_obj(Path::new("assets/1m_cube.obj"));
    let cube_10m = obj_loader::load_obj(Path::new("assets/10m_cube.obj"));
    let cube_30m = obj_loader::load_obj(Path::new("assets/30m_cube.obj"));

    // 1m cube: center Y=0.5 so bottom is Y=0. Place at Z=10.
    objects.push(spawn(
        device, &cube_1m, "cube_1m",
        DVec3::new(0.0, 0.5, 10.0), Quat::IDENTITY, 1.0, id,
    ));
    id += 1;

    // 10m cube: 1m cube far edge at Z=10.5, +10m gap → near edge at Z=20.5, center at Z=25.5
    objects.push(spawn(
        device, &cube_10m, "cube_10m",
        DVec3::new(0.0, 5.0, 25.5), Quat::IDENTITY, 1.0, id,
    ));
    id += 1;

    // 30m cube: 10m cube far edge at Z=30.5, +10m gap → near edge at Z=40.5, center at Z=55.5
    objects.push(spawn(
        device, &cube_30m, "cube_30m",
        DVec3::new(0.0, 15.0, 55.5), Quat::IDENTITY, 1.0, id,
    ));*/
    let _ = id;

    objects
}
