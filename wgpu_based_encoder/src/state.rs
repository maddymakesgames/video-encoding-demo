use std::{num::NonZeroU32, sync::mpsc::Sender, thread::JoinHandle, time::Instant};

use cgmath::{prelude::*, Matrix4, Quaternion, Vector3};
use image::{Bgra, ImageBuffer};
use stream_encoder::{start_encoding, VideoSettings};
use wgpu::{
    include_wgsl,
    util::{BufferInitDescriptor, DeviceExt},
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferAddress, BufferBindingType,
    BufferDescriptor, BufferUsages, CompareFunction, DepthBiasState, DepthStencilState, Extent3d,
    ImageCopyBuffer, ImageCopyTexture, ImageDataLayout, LoadOp, Maintain, MapMode, Operations,
    Origin3d, RenderPassDepthStencilAttachment, SamplerBindingType, ShaderStages, StencilState,
    TextureAspect, TextureSampleType, TextureUsages, TextureViewDimension,
};
use winit::{dpi::PhysicalSize, event::WindowEvent, window::Window};

use crate::{
    camera::{Camera, CameraUniform},
    controller::CameraController,
    texture::Texture,
};

pub struct State {
    instances: Vec<Instance>,
    instance_buffer: Buffer,
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub(crate) size: winit::dpi::PhysicalSize<u32>,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    num_indicies: u32,
    diffuse_bind_group: BindGroup,
    _diffuse_texture: Texture,
    camera: Camera,
    camera_uniform: CameraUniform,
    camera_buffer: Buffer,
    camera_bind_group: BindGroup,
    camera_controller: CameraController,
    depth_texture: Texture,
    frame_sender: Sender<ImageBuffer<Bgra<u8>, Vec<u8>>>,
    frame_texture: Texture,
    frame_buffer: Buffer,
    frame_thread: JoinHandle<()>,
    frame_time: Instant,
    frame_num: u64,
}

impl State {
    pub async fn new(window: &Window) -> Self {
        let size = window.inner_size();

        #[cfg(not(feature = "gl"))]
        let instance = wgpu::Instance::new(wgpu::Backends::VULKAN);
        #[cfg(feature = "gl")]
        let instance = wgpu::Instance::new(wgpu::Backends::GL);
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                    label: Some("Main device queue"),
                },
                None,
            )
            .await
            .unwrap();

        let texture = Texture::from_bytes(
            &device,
            &queue,
            include_bytes!("rusty_quartz.png"),
            Some("Texture"),
        )
        .unwrap();

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("texture bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let diffuse_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture bind group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&texture.sampler),
                },
            ],
        });

        let config = wgpu::SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_DST,
            format: surface.get_preferred_format(&adapter).unwrap(),
            width: 256 * (size.width / 256),
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
        };

        let camera = Camera {
            eye: (0.0, 1.0, 2.0).into(),
            target: (0.0, 0.0, 0.0).into(),
            up: Vector3::unit_y(),
            aspect: config.width as f32 / config.height as f32,
            fovy: 45.0,
            znear: 0.1,
            zfar: 100.0,
        };

        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(&camera);

        let depth_texture = Texture::create_depth_texture(&device, &config, Some("depth texture"));

        let camera_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("camera buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("camera group layout"),
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::VERTEX,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Texture {
                            sample_type: TextureSampleType::Depth,
                            view_dimension: TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 2,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Sampler(SamplerBindingType::Comparison),
                        count: None,
                    },
                ],
            });

        let camera_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("camera group"),
            layout: &camera_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&depth_texture.view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Sampler(&depth_texture.sampler),
                },
            ],
        });

        surface.configure(&device, &config);

        let shader = device.create_shader_module(&include_wgsl!("./shader.wgsl"));

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("pipeline"),
                bind_group_layouts: &[&texture_bind_group_layout, &camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc(), InstanceRaw::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "frag_main",
                targets: &[wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: CompareFunction::Less,
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let camera_controller = CameraController::new(0.2);

        let instances = (0..NUM_INSTANCES_PER_ROW)
            .flat_map(|z| {
                (0..NUM_INSTANCES_PER_ROW).map(move |x| {
                    let position = Vector3::new(x as f32, 0.0, z as f32) - INSTANCE_DISPLACEMENT;

                    let rotation = if position.is_zero() {
                        Quaternion::from_axis_angle(Vector3::unit_z(), cgmath::Deg(0.0))
                    } else {
                        Quaternion::from_axis_angle(position.normalize(), cgmath::Deg(45.0))
                    };

                    Instance { position, rotation }
                })
            })
            .collect::<Vec<_>>();

        let instance_data = instances.iter().map(Instance::to_raw).collect::<Vec<_>>();
        let instance_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(&instance_data),
            usage: BufferUsages::VERTEX,
        });

        let (frame_thread, frame_sender) = Self::init_encoder(&size);

        let frame_texture = Texture::create_encoding_frame(&device, &config, Some("encoder frame"));

        let frame_buffer_size =
            (std::mem::size_of::<u32>() as u32 * 256 * (size.width / 256) * config.height)
                as BufferAddress;
        let frame_buffer_desc = BufferDescriptor {
            size: frame_buffer_size,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            label: None,
            mapped_at_creation: false,
        };

        let frame_buffer = device.create_buffer(&frame_buffer_desc);

        Self {
            config,
            device,
            queue,
            size,
            surface,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indicies: INDICES.len() as u32,
            diffuse_bind_group,
            _diffuse_texture: texture,
            camera,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
            camera_controller,
            instances,
            instance_buffer,
            depth_texture,
            frame_sender,
            frame_texture,
            frame_buffer,
            frame_thread,
            frame_time: Instant::now(),
            frame_num: 0,
        }
    }

    fn init_encoder(
        size: &PhysicalSize<u32>,
    ) -> (JoinHandle<()>, Sender<ImageBuffer<Bgra<u8>, Vec<u8>>>) {
        let mut video_settings = VideoSettings::new(
            crate::FRAME_RATE as u64,
            256 * (size.width / 256),
            size.height,
        );
        video_settings
            .encoder_settings
            .insert("pass".to_owned(), "qual".to_owned());
        video_settings
            .encoder_settings
            .insert("speed-preset".to_owned(), "slow".to_owned());

        // We're using Bgra images, with data stored in Vecs and want a 120 frame buffer
        start_encoding::<Bgra<u8>, Vec<u8>, 120>("./recording.mp4", video_settings)
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth_texture =
            Texture::create_depth_texture(&self.device, &self.config, Some("depth texture"));
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        self.camera_controller.process_events(event)
    }

    pub fn update(&mut self) {
        self.camera_controller.update_camera(&mut self.camera);
        self.camera_uniform.update_view_proj(&self.camera);
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.camera_uniform]),
        )
    }

    pub async fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        // get surface texture view
        let output = self.surface.get_current_texture()?;
        #[cfg(feature = "gl")]
        let view = output
            .texture
            .create_view(&TextureViewDescriptor::default());

        // Initialize command
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // make render pass
        let mut encode_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[wgpu::RenderPassColorAttachment {
                view: &self.frame_texture.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.,
                    }),
                    store: true,
                },
            }],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &self.depth_texture.view,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(1.0),
                    store: true,
                }),
                stencil_ops: None,
            }),
        });

        encode_pass.set_pipeline(&self.render_pipeline);
        encode_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
        encode_pass.set_bind_group(1, &self.camera_bind_group, &[]);
        encode_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        encode_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        encode_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        encode_pass.draw_indexed(0..self.num_indicies, 0, 0..self.instances.len() as _);

        drop(encode_pass);

        #[cfg(features = "gl")]
        {
            // With OpenGL (and possibly other backends) we can't copy to the surface
            // so we have to render the scene twice
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.,
                        }),
                        store: true,
                    },
                }],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(1.0),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });
            // use render pass
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
            render_pass.set_bind_group(1, &self.camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.num_indicies, 0, 0..self.instances.len() as _);

            drop(render_pass);
        }

        encoder.copy_texture_to_buffer(
            ImageCopyTexture {
                aspect: TextureAspect::All,
                texture: &self.frame_texture.texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
            },
            ImageCopyBuffer {
                buffer: &self.frame_buffer,
                layout: ImageDataLayout {
                    offset: 0,
                    bytes_per_row: NonZeroU32::new(
                        std::mem::size_of::<u32>() as u32 * 256 * (self.config.width / 256),
                    ),
                    rows_per_image: NonZeroU32::new(self.config.height),
                },
            },
            Extent3d {
                width: 256 * (self.config.width / 256),
                height: self.config.height,
                depth_or_array_layers: 1,
            },
        );

        // On vulkan (and possibly other backends)
        // we can copy directly to the surface texture+-
        #[cfg(not(features = "gl"))]
        encoder.copy_texture_to_texture(
            ImageCopyTexture {
                aspect: TextureAspect::All,
                texture: &self.frame_texture.texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
            },
            ImageCopyTexture {
                aspect: TextureAspect::All,
                texture: &output.texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
            },
            Extent3d {
                width: 256 * (self.config.width / 256),
                height: self.config.height,
                depth_or_array_layers: 1,
            },
        );

        // submit the command and present
        self.queue.submit(Some(encoder.finish()));
        output.present();

        let buffer_slice = self.frame_buffer.slice(..);
        let mapping = buffer_slice.map_async(MapMode::Read);
        self.device.poll(Maintain::Wait);
        mapping.await.unwrap();

        let data = buffer_slice.get_mapped_range();

        let buffer_size = std::mem::size_of::<u32>()
            * 256
            * (self.config.width as usize / 256)
            * self.config.height as usize;

        let mut bytes = Vec::with_capacity(buffer_size);
        unsafe {
            data.as_ptr()
                .copy_to_nonoverlapping(bytes.as_mut_ptr(), buffer_size);
            bytes.set_len(buffer_size)
        }

        let frame_sender = self.frame_sender.clone();
        let width = 256 * (self.config.width / 256);
        let height = self.config.height;
        let curr_time = Instant::now();

        // We could be drawing faster than we want to encode, so we only encode on multiples of our framerate
        if curr_time.duration_since(self.frame_time).as_millis()
            >= (1000 / crate::FRAME_RATE as u128)
        {
            self.frame_time = curr_time;

            // Technically I think this could lead to a race condition
            // if somehow from_raw took an insane amount of time
            // and the next frame didn't take as long
            std::thread::spawn(move || {
                let buffer = ImageBuffer::<Bgra<u8>, _>::from_raw(width, height, bytes).unwrap();

                match frame_sender.send(buffer) {
                    Ok(_) => {}
                    Err(_) => eprintln!("tried to encode thread after closing the window"),
                };
            });
        }
        drop(data);
        drop(buffer_slice);

        self.frame_num += 1;
        self.frame_buffer.unmap();

        Ok(())
    }

    pub fn close(&mut self) {
        let prev = std::mem::replace(&mut self.frame_sender, std::sync::mpsc::channel().0);
        drop(prev);
        let encoder_thread = std::mem::replace(&mut self.frame_thread, std::thread::spawn(|| {}));
        encoder_thread.join().unwrap();
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [-0.0868241, 0.49240386, 0.0],
        tex_coords: [0.4131759, 0.99240386],
    }, // A
    Vertex {
        position: [-0.49513406, 0.06958647, 0.0],
        tex_coords: [0.0048659444, 0.56958647],
    }, // B
    Vertex {
        position: [-0.21918549, -0.44939706, 0.0],
        tex_coords: [0.28081453, 0.05060294],
    }, // C
    Vertex {
        position: [0.35966998, -0.3473291, 0.0],
        tex_coords: [0.85967, 0.1526709],
    }, // D
    Vertex {
        position: [0.44147372, 0.2347359, 0.0],
        tex_coords: [0.9414737, 0.7347359],
    }, // E
];
const INDICES: &[u16] = &[0, 1, 4, 1, 2, 4, 2, 3, 4];

impl Vertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

struct Instance {
    position: Vector3<f32>,
    rotation: Quaternion<f32>,
}

impl Instance {
    fn to_raw(&self) -> InstanceRaw {
        InstanceRaw {
            model: (Matrix4::from_translation(self.position) * Matrix4::from(self.rotation)).into(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceRaw {
    model: [[f32; 4]; 4],
}

const NUM_INSTANCES_PER_ROW: u32 = 10;
const INSTANCE_DISPLACEMENT: Vector3<f32> = Vector3::new(
    NUM_INSTANCES_PER_ROW as f32 * 0.5,
    0.0,
    NUM_INSTANCES_PER_ROW as f32 * 0.5,
);

impl InstanceRaw {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
            // We need to switch from using a step mode of Vertex to Instance
            // This means that our shaders will only change to use the next
            // instance when the shader starts processing a new instance
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    // While our vertex shader only uses locations 0, and 1 now, in later tutorials we'll
                    // be using 2, 3, and 4, for Vertex. We'll start at slot 5 not conflict with them later
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // A mat4 takes up 4 vertex slots as it is technically 4 vec4s. We need to define a slot
                // for each vec4. We'll have to reassemble the mat4 in
                // the shader.
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}
