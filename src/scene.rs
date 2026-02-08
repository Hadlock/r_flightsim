use glam::{DVec3, Mat4, Quat};
use std::path::Path;
use wgpu::util::DeviceExt;

use crate::coords::ENUFrame;
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

/// Load the Ki-61 aircraft model as a SceneObject.
/// Position and rotation are set to defaults — caller updates them each frame.
pub fn load_aircraft_object(device: &wgpu::Device, object_id: u32) -> SceneObject {
    let mesh = obj_loader::load_obj(Path::new(
        "assets/14082_WWII_Plane_Japan_Kawasaki_Ki-61_v1_L2.obj",
    ));
    // Ki-61: 12m wingspan, OBJ wingspan extent is ~2.2019 units
    let scale = 12.0 / 2.2019;
    spawn(
        device,
        &mesh,
        "aircraft",
        DVec3::ZERO,
        Quat::IDENTITY,
        scale as f32,
        object_id,
    )
}

/// Load reference objects (teapots) placed near the aircraft starting position.
pub fn load_scene(device: &wgpu::Device, ref_pos: DVec3, enu: &ENUFrame) -> Vec<SceneObject> {
    let teapot_mesh = obj_loader::load_obj(Path::new("assets/teapot.obj"));

    // Scale teapot to ~2m tall for visibility as runway markers
    let teapot_scale = 2.0 / 6.434;

    let mut objects = Vec::new();
    let mut id = 10u32;

    // Place teapots along the runway (north direction) as visual reference
    for i in 0..10 {
        let north_offset = (i as f64) * 100.0; // every 100m
        let offset_enu = DVec3::new(10.0, north_offset, 0.0); // 10m east of centerline
        let pos = ref_pos + enu.enu_to_ecef(offset_enu);
        objects.push(spawn(
            device,
            &teapot_mesh,
            &format!("teapot_{}", i),
            pos,
            Quat::IDENTITY,
            teapot_scale as f32,
            id,
        ));
        id += 1;
    }

    let _ = id;
    objects
}
