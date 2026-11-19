use wgpu::{util::DeviceExt, *};
use winit::window::Window;
use bytemuck::cast_slice;

use crate::{
    camera::{Camera, CameraUniform},
    material::{MaterialUniform, FLAG_HAS_ALBEDO, FLAG_HAS_METALLIC_ROUGH, FLAG_HAS_NORMAL_MAP},
    mesh::GltfScene,
    pipeline::{create_depth_texture, create_pbr_pipeline, DEPTH_FORMAT},
};

// Matches LightUniform in pbr.wgsl
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct LightUniform {
    direction: [f32; 4],
    color: [f32; 4],
    ambient: [f32; 4],
}

// Matches ModelUniform in pbr.wgsl — 2× mat4
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ModelUniform {
    model: [[f32; 4]; 4],
    normal_mat: [[f32; 4]; 4],
}

pub struct GtmRenderer {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,

    depth_view: TextureView,
    _depth_tex: Texture,

    camera_buf: Buffer,
    camera_bgl: BindGroupLayout,
    camera_bg: BindGroup,

    light_buf: Buffer,
    light_bgl: BindGroupLayout,
    light_bg: BindGroup,

    model_buf: Buffer,
    model_bgl: BindGroupLayout,
    model_bg: BindGroup,

    material_bgl: BindGroupLayout,
    default_material_buf: Buffer,
    default_material_bg: BindGroup,

    pipeline: RenderPipeline,

    pub camera: Camera,
    scenes: Vec<GltfScene>,
}

impl GtmRenderer {
    pub async fn new(window: std::sync::Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::VULKAN | Backends::GL,
            ..Default::default()
        });

        let surface = instance.create_surface(window).expect("surface");

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("adapter");

        let (device, queue) = adapter
            .request_device(
                &DeviceDescriptor {
                    label: Some("gtm_device"),
                    required_features: Features::empty(),
                    required_limits: Limits::downlevel_webgl2_defaults()
                        .using_resolution(adapter.limits()),
                    memory_hints: MemoryHints::Performance,
                },
                None,
            )
            .await
            .expect("device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let (depth_tex, depth_view) =
            create_depth_texture(&device, config.width, config.height);

        // ── Camera BGL / buffer / BG ─────────────────────────────────────
        let camera_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("camera_bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let camera = Camera::new(config.width as f32 / config.height as f32);
        let camera_data = camera.to_uniform();
        let camera_buf = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("camera_buf"),
            contents: cast_slice(&[camera_data]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        let camera_bg = device.create_bind_group(&BindGroupDescriptor {
            label: Some("camera_bg"),
            layout: &camera_bgl,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        // ── Light BGL / buffer / BG ──────────────────────────────────────
        let light_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("light_bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // Default Florida noon sun from gtm-lighting palette
        let light_data = LightUniform {
            direction: [-0.3, -0.7, -0.5, 3.5],  // dir + intensity
            color: [1.0, 0.95, 0.80, 0.0],        // warm sun
            ambient: [0.12, 0.15, 0.22, 1.0],     // blue-teal sky ambient
        };
        let light_buf = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("light_buf"),
            contents: cast_slice(&[light_data]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        let light_bg = device.create_bind_group(&BindGroupDescriptor {
            label: Some("light_bg"),
            layout: &light_bgl,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: light_buf.as_entire_binding(),
            }],
        });

        // ── Model BGL / buffer / BG (identity default) ───────────────────
        let model_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("model_bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        use glam::Mat4;
        let ident = Mat4::IDENTITY.to_cols_array_2d();
        let model_data = ModelUniform { model: ident, normal_mat: ident };
        let model_buf = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("model_buf"),
            contents: cast_slice(&[model_data]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        let model_bg = device.create_bind_group(&BindGroupDescriptor {
            label: Some("model_bg"),
            layout: &model_bgl,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: model_buf.as_entire_binding(),
            }],
        });

        // ── Material BGL ─────────────────────────────────────────────────
        let material_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("material_bgl"),
            entries: &[
                // uniform
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // albedo tex + sampler
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                // normal tex + sampler
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                // metallic-roughness tex + sampler
                BindGroupLayoutEntry {
                    binding: 5,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 6,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // 1×1 white fallback textures for slots without maps
        let white_data: [u8; 4] = [255, 255, 255, 255];
        let flat_normal_data: [u8; 4] = [128, 128, 255, 255]; // neutral normal map
        let white_tex = make_1x1_texture(&device, &queue, &white_data, "white");
        let flat_tex  = make_1x1_texture(&device, &queue, &flat_normal_data, "flat_normal");

        let default_sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("default_sampler"),
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::Repeat,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
            ..Default::default()
        });

        let default_mat = MaterialUniform::default();
        let default_material_buf = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("default_material_buf"),
            contents: cast_slice(&[default_mat]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let white_view    = white_tex.create_view(&TextureViewDescriptor::default());
        let flat_view     = flat_tex.create_view(&TextureViewDescriptor::default());

        let default_material_bg = device.create_bind_group(&BindGroupDescriptor {
            label: Some("default_material_bg"),
            layout: &material_bgl,
            entries: &[
                BindGroupEntry { binding: 0, resource: default_material_buf.as_entire_binding() },
                BindGroupEntry { binding: 1, resource: BindingResource::TextureView(&white_view) },
                BindGroupEntry { binding: 2, resource: BindingResource::Sampler(&default_sampler) },
                BindGroupEntry { binding: 3, resource: BindingResource::TextureView(&flat_view)  },
                BindGroupEntry { binding: 4, resource: BindingResource::Sampler(&default_sampler) },
                BindGroupEntry { binding: 5, resource: BindingResource::TextureView(&white_view) },
                BindGroupEntry { binding: 6, resource: BindingResource::Sampler(&default_sampler) },
            ],
        });

        // ── Pipeline ─────────────────────────────────────────────────────
        let pipeline = create_pbr_pipeline(
            &device,
            format,
            &camera_bgl,
            &light_bgl,
            &model_bgl,
            &material_bgl,
        );

        Self {
            surface, device, queue, config,
            depth_view, _depth_tex: depth_tex,
            camera_buf, camera_bgl, camera_bg,
            light_buf, light_bgl, light_bg,
            model_buf, model_bgl, model_bg,
            material_bgl, default_material_buf, default_material_bg,
            pipeline,
            camera,
            scenes: Vec::new(),
        }
    }

    pub fn device(&self) -> &Device { &self.device }

    pub fn queue(&self) -> &Queue { &self.queue }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 { return; }
        self.config.width  = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        let (dt, dv) = create_depth_texture(&self.device, width, height);
        self._depth_tex = dt;
        self.depth_view = dv;
        self.camera.aspect = width as f32 / height as f32;
    }

    /// Load a .glb from bytes and push to scene list.
    pub fn load_gltf(&mut self, bytes: &[u8]) -> anyhow::Result<usize> {
        let scene = GltfScene::from_bytes(&self.device, &self.queue, bytes)?;
        self.scenes.push(scene);
        Ok(self.scenes.len() - 1)
    }

    pub fn update(&mut self) {
        // Push updated camera uniform
        let cam_data = self.camera.to_uniform();
        self.queue.write_buffer(&self.camera_buf, 0, cast_slice(&[cam_data]));
    }

    pub fn render(&mut self) -> Result<(), SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view   = output.texture.create_view(&TextureViewDescriptor::default());

        let mut enc = self.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("render_enc"),
        });

        {
            let mut pass = enc.begin_render_pass(&RenderPassDescriptor {
                label: Some("pbr_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color {
                            r: 0.47, g: 0.65, b: 0.80, a: 1.0, // Florida sky blue
                        }),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(Operations { load: LoadOp::Clear(1.0), store: StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bg, &[]);
            pass.set_bind_group(1, &self.light_bg, &[]);
            pass.set_bind_group(2, &self.model_bg, &[]);
            pass.set_bind_group(3, &self.default_material_bg, &[]);

            for scene in &self.scenes {
                for mesh in &scene.meshes {
                    pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                    pass.set_index_buffer(mesh.index_buffer.slice(..), IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                }
            }
        }

        self.queue.submit(std::iter::once(enc.finish()));
        output.present();
        Ok(())
    }
}

fn make_1x1_texture(
    device: &Device,
    queue: &Queue,
    rgba: &[u8; 4],
    label: &str,
) -> Texture {
    let tex = device.create_texture(&TextureDescriptor {
        label: Some(label),
        size: Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8UnormSrgb,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        tex.as_image_copy(),
        rgba,
        ImageDataLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: None },
        Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    tex
}
