use bytemuck::{Pod, Zeroable};
use glam::Mat4;

use crate::obj_loader::Vertex;
use crate::scene::SceneObject;

/// Minimum alignment for dynamic uniform buffer offsets (256 bytes is the wgpu default).
const UNIFORM_ALIGN: u64 = 256;

/// Max objects we can render in one frame.
const MAX_OBJECTS: u64 = 2048;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GeometryUniforms {
    mvp: [[f32; 4]; 4],
    model_view: [[f32; 4]; 4],
    object_id: u32,
    _pad: [u32; 3],
}

pub struct Renderer {
    // Geometry pass
    geometry_pipeline: wgpu::RenderPipeline,
    geometry_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,

    // Edge detection pass
    edge_pipeline: wgpu::RenderPipeline,
    edge_bind_group_layout: wgpu::BindGroupLayout,

    // Offscreen textures
    depth_texture: wgpu::TextureView,
    normal_texture: wgpu::TextureView,
    object_id_texture: wgpu::TextureView,

    // For edge pass sampling
    edge_bind_group: wgpu::BindGroup,

    pub width: u32,
    pub height: u32,
}

impl Renderer {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        // Load shaders
        let geometry_shader =
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Geometry Shader"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("../shaders/geometry.wgsl").into(),
                ),
            });

        let edge_shader =
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Edge Detection Shader"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("../shaders/edge_detect.wgsl").into(),
                ),
            });

        // Geometry pass bind group layout (uniform buffer with dynamic offset)
        let geometry_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Geometry Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<GeometryUniforms>() as u64,
                        ),
                    },
                    count: None,
                }],
            });

        let geometry_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Geometry Pipeline Layout"),
                bind_group_layouts: &[&geometry_bind_group_layout],
                push_constant_ranges: &[],
            });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // normal
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        };

        let geometry_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Geometry Pipeline"),
                layout: Some(&geometry_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &geometry_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[vertex_layout],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &geometry_shader,
                    entry_point: Some("fs_main"),
                    targets: &[
                        // Normal texture
                        Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::Rgba16Float,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                        // Object ID texture
                        Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::R32Uint,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                    ],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview: None,
                cache: None,
            });

        // Uniform buffer for geometry pass — one 256-byte-aligned slot per object
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Geometry Uniform Buffer"),
            size: UNIFORM_ALIGN * MAX_OBJECTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Single bind group shared by all objects (dynamic offset selects the slot)
        let geometry_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Geometry Bind Group"),
            layout: &geometry_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<GeometryUniforms>() as u64),
                }),
            }],
        });

        // Edge detection bind group layout
        let edge_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Edge Bind Group Layout"),
                entries: &[
                    // depth texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // normal texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // object ID texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Uint,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let edge_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Edge Pipeline Layout"),
                bind_group_layouts: &[&edge_bind_group_layout],
                push_constant_ranges: &[],
            });

        let edge_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Edge Detection Pipeline"),
                layout: Some(&edge_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &edge_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[], // fullscreen quad via vertex_index
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &edge_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: surface_format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
                cache: None,
            });

        // Create offscreen textures
        let (depth_view, normal_view, object_id_view) =
            Self::create_offscreen_textures(device, width, height);

        let edge_bind_group = Self::create_edge_bind_group(
            device,
            &edge_bind_group_layout,
            &depth_view,
            &normal_view,
            &object_id_view,
        );

        Self {
            geometry_pipeline,
            geometry_bind_group,
            uniform_buffer,
            edge_pipeline,
            edge_bind_group_layout,
            depth_texture: depth_view,
            normal_texture: normal_view,
            object_id_texture: object_id_view,
            edge_bind_group,
            width,
            height,
        }
    }

    fn create_offscreen_textures(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::TextureView, wgpu::TextureView, wgpu::TextureView) {
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let normal_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Normal Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let object_id_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Object ID Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let depth_view = depth_texture.create_view(&Default::default());
        let normal_view = normal_texture.create_view(&Default::default());
        let object_id_view = object_id_texture.create_view(&Default::default());

        (depth_view, normal_view, object_id_view)
    }

    fn create_edge_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        depth_view: &wgpu::TextureView,
        normal_view: &wgpu::TextureView,
        object_id_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Edge Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(object_id_view),
                },
            ],
        })
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;

        let (depth_view, normal_view, object_id_view) =
            Self::create_offscreen_textures(device, width, height);

        self.edge_bind_group = Self::create_edge_bind_group(
            device,
            &self.edge_bind_group_layout,
            &depth_view,
            &normal_view,
            &object_id_view,
        );

        self.depth_texture = depth_view;
        self.normal_texture = normal_view;
        self.object_id_texture = object_id_view;
    }

    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_view: &wgpu::TextureView,
        objects: &[SceneObject],
        view: Mat4,
        proj: Mat4,
        camera_pos: glam::DVec3,
    ) {
        // Upload per-object uniforms into aligned slots BEFORE encoding any passes.
        for (i, obj) in objects.iter().enumerate() {
            let model = obj.model_matrix_relative_to(camera_pos);
            let model_view = view * model;
            let mvp = proj * model_view;

            let uniforms = GeometryUniforms {
                mvp: mvp.to_cols_array_2d(),
                model_view: model_view.to_cols_array_2d(),
                object_id: obj.object_id,
                _pad: [0; 3],
            };

            let offset = i as u64 * UNIFORM_ALIGN;
            queue.write_buffer(&self.uniform_buffer, offset, bytemuck::bytes_of(&uniforms));
        }

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Pass 1: Geometry pass
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Geometry Pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.normal_texture,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.5,
                                g: 0.5,
                                b: 1.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.object_id_texture,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.geometry_pipeline);

            for (i, obj) in objects.iter().enumerate() {
                let dyn_offset = (i as u64 * UNIFORM_ALIGN) as u32;
                pass.set_bind_group(0, &self.geometry_bind_group, &[dyn_offset]);
                pass.set_vertex_buffer(0, obj.vertex_buf.slice(..));
                pass.set_index_buffer(obj.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..obj.index_count, 0, 0..1);
            }
        }

        // Pass 2: Edge detection (fullscreen quad)
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Edge Detection Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.10,
                            g: 0.20,
                            b: 0.30,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.edge_pipeline);
            pass.set_bind_group(0, &self.edge_bind_group, &[]);
            pass.draw(0..3, 0..1); // fullscreen triangle
        }

        queue.submit(std::iter::once(encoder.finish()));
    }
}
