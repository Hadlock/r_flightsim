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

/// Shared mesh data that multiple SceneObjects can reference
struct SharedMesh {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
}

fn create_shared_mesh(device: &wgpu::Device, mesh: &MeshData) -> SharedMesh {
    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Shared Vertex Buffer"),
        contents: bytemuck::cast_slice(&mesh.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Shared Index Buffer"),
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    SharedMesh {
        vertex_buf,
        index_buf,
        index_count: mesh.indices.len() as u32,
    }
}

pub fn load_scene(device: &wgpu::Device) -> Vec<SceneObject> {
    let obj_path = Path::new("assets/teapot.obj");
    let mesh = obj_loader::load_obj(obj_path);

    // Create one shared GPU buffer set
    let shared = create_shared_mesh(device, &mesh);

    let mut objects = Vec::new();

    // Place 10 teapots in a grid pattern
    let positions: [(f64, f64, f64); 10] = [
        (0.0, 0.0, 0.0),
        (8.0, 0.0, 0.0),
        (-8.0, 0.0, 0.0),
        (0.0, 0.0, 8.0),
        (0.0, 0.0, -8.0),
        (8.0, 0.0, 8.0),
        (-8.0, 0.0, 8.0),
        (8.0, 0.0, -8.0),
        (-8.0, 0.0, -8.0),
        (0.0, 4.0, 0.0),
    ];

    for (i, &(x, y, z)) in positions.iter().enumerate() {
        // Each object needs its own buffer references for rendering,
        // but we can share the underlying data by creating new buffers
        // pointing to the same data. For 10 objects this is fine.
        let vertex_buf = if i == 0 {
            // Reuse the shared buffer for the first object
            shared.vertex_buf.clone()
        } else {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Teapot {} Vertex Buffer", i)),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        };

        let index_buf = if i == 0 {
            shared.index_buf.clone()
        } else {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Teapot {} Index Buffer", i)),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            })
        };

        objects.push(SceneObject {
            name: format!("teapot_{}", i),
            vertex_buf,
            index_buf,
            index_count: shared.index_count,
            world_pos: DVec3::new(x, y, z),
            rotation: Quat::IDENTITY,
            scale: 1.0,
            object_id: (i + 1) as u32, // 0 reserved for "no object"
            edges_enabled: true,
        });
    }

    objects
}
