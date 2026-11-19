use wgpu::*;
use crate::mesh::Vertex;

pub const DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;

pub fn create_pbr_pipeline(
    device: &Device,
    surface_format: TextureFormat,
    camera_bgl: &BindGroupLayout,
    light_bgl: &BindGroupLayout,
    model_bgl: &BindGroupLayout,
    material_bgl: &BindGroupLayout,
) -> RenderPipeline {
    let shader_src = include_str!("shaders/pbr.wgsl");
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("pbr_shader"),
        source: ShaderSource::Wgsl(shader_src.into()),
    });

    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("pbr_pipeline_layout"),
        bind_group_layouts: &[camera_bgl, light_bgl, model_bgl, material_bgl],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("pbr_pipeline"),
        layout: Some(&layout),
        vertex: VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[Vertex::layout()],
            compilation_options: Default::default(),
        },
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(ColorTargetState {
                format: surface_format,
                blend: Some(BlendState::REPLACE),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            cull_mode: Some(Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: CompareFunction::Less,
            stencil: StencilState::default(),
            bias: DepthBiasState::default(),
        }),
        multisample: MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

pub fn create_depth_texture(
    device: &Device,
    width: u32,
    height: u32,
) -> (Texture, TextureView) {
    let tex = device.create_texture(&TextureDescriptor {
        label: Some("depth_texture"),
        size: Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&TextureViewDescriptor::default());
    (tex, view)
}
