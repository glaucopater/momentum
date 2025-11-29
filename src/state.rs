use winit::window::Window;
use wgpu::util::DeviceExt;
use crate::texture;
use glam::{Mat4, Vec3};
use std::path::{Path, PathBuf};

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
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

const VERTICES: &[Vertex] = &[
    Vertex { position: [-1.0, 1.0, 0.0], tex_coords: [0.0, 0.0] },
    Vertex { position: [-1.0, -1.0, 0.0], tex_coords: [0.0, 1.0] },
    Vertex { position: [1.0, -1.0, 0.0], tex_coords: [1.0, 1.0] },
    Vertex { position: [1.0, 1.0, 0.0], tex_coords: [1.0, 0.0] },
];

const INDICES: &[u16] = &[
    0, 1, 2,
    0, 2, 3,
];

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    scale: [f32; 2],
    padding: [f32; 2], // Padding to align to 16 bytes (mat4 is 64, vec2 is 8, need 8 more)
}

impl CameraUniform {
    fn new() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            scale: [1.0, 1.0],
            padding: [0.0, 0.0],
        }
    }

    fn update_view_proj(&mut self, camera: &Camera, image_aspect: f32) {
        let view = Mat4::look_at_rh(
            Vec3::new(camera.x, camera.y, 1.0),
            Vec3::new(camera.x, camera.y, 0.0),
            Vec3::Y,
        );
        
        let proj = Mat4::orthographic_rh(
            -camera.aspect * camera.zoom, 
            camera.aspect * camera.zoom, 
            -camera.zoom, 
            camera.zoom, 
            0.1, 
            100.0
        );
        
        self.view_proj = (proj * view).to_cols_array_2d();
        
        // If image_aspect > 1.0 (wider), we scale X.
        // If image_aspect < 1.0 (taller), we scale Y?
        // Actually, let's just make the quad size match the aspect ratio.
        // Quad is 2x2 (-1 to 1).
        // We want it to be (2*aspect) x 2.
        self.scale = [image_aspect, 1.0];
    }
}

struct Camera {
    x: f32,
    y: f32,
    zoom: f32,
    aspect: f32,
}

pub struct State<'a> {
    pub surface: wgpu::Surface<'a>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub window: &'a Window,
    pub render_pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
    pub diffuse_bind_group: wgpu::BindGroup,
    pub diffuse_texture: texture::Texture,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    
    camera: Camera,
    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    
    mouse_pressed: bool,
    last_mouse_pos: Option<(f64, f64)>,
    image_aspect: f32,
    
    // UI Data
    load_time: std::time::Duration,
    memory_usage: u64,
    exif_data: std::collections::HashMap<String, String>,
    
    // Navigation
    navigator: crate::navigator::Navigator,
}

impl<'a> State<'a> {
    pub async fn new(window: &'a Window) -> State<'a> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: adapter.limits(),
                },
                None,
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        // Load a default texture
        let diffuse_image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(1, 1, image::Rgba([50, 50, 50, 255])));
        let diffuse_texture = texture::Texture::from_image(&device, &queue, &diffuse_image, Some("diffuse_texture")).unwrap();

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
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
                label: Some("texture_bind_group_layout"),
            });

        let diffuse_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&diffuse_texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&diffuse_texture.sampler),
                },
            ],
            label: Some("diffuse_bind_group"),
        });

        // Camera setup
        let camera = Camera {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
            aspect: config.width as f32 / config.height as f32,
        };
        
        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(&camera, 1.0);

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
            label: Some("camera_bind_group_layout"),
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("camera_bind_group"),
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&texture_bind_group_layout, &camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
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
            depth_stencil: None,
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

        let num_indices = INDICES.len() as u32;

        Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices,
            diffuse_bind_group,
            diffuse_texture,
            texture_bind_group_layout,
            camera,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
            mouse_pressed: false,
            last_mouse_pos: None,
            image_aspect: 1.0,
            load_time: std::time::Duration::from_secs(0),
            memory_usage: 0,
            exif_data: std::collections::HashMap::new(),
            navigator: crate::navigator::Navigator::new(),
        }
    }

    pub fn set_image(&mut self, loaded_image: crate::loader::LoadedImage) {
        let img = loaded_image.image;
        let texture = crate::texture::Texture::from_image(&self.device, &self.queue, &img, Some("Image")).unwrap();
        
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.texture_bind_group_layout,
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
            label: Some("diffuse_bind_group"),
        });

        self.diffuse_texture = texture;
        self.diffuse_bind_group = bind_group;
        
        // Update aspect ratio
        self.image_aspect = img.width() as f32 / img.height() as f32;
        
        // Reset camera
        self.camera.x = 0.0;
        self.camera.y = 0.0;
        self.camera.zoom = 1.0;
        
        // Update UI data
        self.load_time = loaded_image.load_time;
        self.memory_usage = (img.width() as u64 * img.height() as u64 * 4) / 1024 / 1024;
        self.exif_data = loaded_image.exif;
        
        // Update window title with info
        self.update_window_title();
        
        self.window.request_redraw();
        
        // Update file list if needed
        self.navigator.update_file_list(&loaded_image.path);
    }
    
    pub fn get_next_image(&self) -> Option<PathBuf> {
        self.navigator.get_next_image()
    }
    
    pub fn get_prev_image(&self) -> Option<PathBuf> {
        self.navigator.get_prev_image()
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            
            self.camera.aspect = self.config.width as f32 / self.config.height as f32;
        }
    }

    pub fn input(&mut self, event: &winit::event::WindowEvent) -> bool {
        use winit::event::*;
        match event {
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.mouse_pressed = *state == ElementState::Pressed;
                true
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.mouse_pressed {
                    if let Some((last_x, last_y)) = self.last_mouse_pos {
                        let dx = position.x - last_x;
                        let dy = position.y - last_y;
                        
                        // Convert screen delta to camera space
                        // Screen width corresponds to 2.0 * aspect * zoom
                        let scale_x = (2.0 * self.camera.aspect * self.camera.zoom) / self.config.width as f32;
                        let scale_y = (2.0 * self.camera.zoom) / self.config.height as f32;
                        
                        self.camera.x -= dx as f32 * scale_x;
                        self.camera.y += dy as f32 * scale_y; // Y is inverted in screen coords vs world
                        
                        self.window.request_redraw();
                    }
                }
                self.last_mouse_pos = Some((position.x, position.y));
                true
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 100.0, // Arbitrary scaling
                };
                
                if scroll > 0.0 {
                    self.camera.zoom *= 0.9;
                } else {
                    self.camera.zoom *= 1.1;
                }
                self.window.request_redraw();
                true
            }
            _ => false,
        }
    }

    pub fn update(&mut self) {
        self.camera_uniform.update_view_proj(&self.camera, self.image_aspect);
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[self.camera_uniform]));
        self.update_window_title();
    }
    
    fn update_window_title(&self) {
        let zoom_pct = (1.0 / self.camera.zoom * 100.0) as i32;
        let mut title = format!("Momemtum - Zoom: {}%", zoom_pct);
        
        if let Some(path) = &self.navigator.current_path {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                title.push_str(&format!(" | {}", name));
            }
        }
        
        if self.load_time.as_millis() > 0 {
            title.push_str(&format!(" | Load: {:.0}ms", self.load_time.as_secs_f64() * 1000.0));
        }
        
        if self.memory_usage > 0 {
            title.push_str(&format!(" | Memory: ~{}MB", self.memory_usage));
        }
        
        if let Some(model) = self.exif_data.get("Model") {
            title.push_str(&format!(" | {}", model));
        }
        
        self.window.set_title(&title);
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
            render_pass.set_bind_group(1, &self.camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
