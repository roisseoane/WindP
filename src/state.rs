use winit::{event::WindowEvent, window::Window};
use wgpu::util::DeviceExt;
use crate::gpu::{GpuContext, texture::Texture};
use crate::pdf::{PdfSystem, render::render_page_to_memory};
use pdfium_render::prelude::*; // Necesario para los tipos PdfDocument

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}

const VERTICES: &[Vertex] = &[
    Vertex { position: [-1.0, 1.0, 0.0], tex_coords: [0.0, 0.0] },
    Vertex { position: [-1.0, -1.0, 0.0], tex_coords: [0.0, 1.0] },
    Vertex { position: 1.0, -1.0, 0.0], tex_coords: [1.0, 1.0] },
    Vertex { position: 1.0, 1.0, 0.0], tex_coords: [1.0, 0.0] },
];

const INDICES: &[u16] = &[0, 1, 2, 2, 3, 0];

pub struct State<'a> {
    gpu: GpuContext,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    diffuse_bind_group: wgpu::BindGroup,
    
    // El documento vive tanto como el PdfSystem en main.rs
    // Usamos Option porque puede que se abra la app sin archivo.
    document: Option<PdfDocument<'a>>, 
    
    current_page: u16,
    num_indices: u32,
}

impl<'a> State<'a> {
    pub async fn new(
        window: &Window, 
        pdf_system: &'a PdfSystem, 
        file_path: Option<String>
    ) -> Self {
        let gpu = GpuContext::new(window).await;
        
        // 1. Intentar cargar el PDF si viene en los argumentos
        let (document, page_bitmap) = if let Some(path) = file_path {
            match pdf_system.open_file(&path) {
                Ok(doc) => {
                    // Renderizamos la pag 0
                    // Usamos unwrap_or para no crashear si falla el renderizado específico
                    let bitmap = render_page_to_memory(&doc, 0, 1.5).unwrap_or_else(|_| {
                        create_fallback_bitmap()
                    });
                    (Some(doc), bitmap)
                },
                Err(e) => {
                    eprintln!("Error al abrir el PDF: {}", e);
                    (None, create_fallback_bitmap())
                }
            }
        } else {
            // Estado inactivo: sin PDF
            (None, create_fallback_bitmap())
        };
        
        // 2. Crear Textura GPU (Con el PDF o el fallback negro)
        let diffuse_texture = Texture::from_bytes(
            &gpu.device, 
            &gpu.queue, 
            &page_bitmap.data, 
            page_bitmap.width, 
            page_bitmap.height, 
            Some("PDF Page")
        ).unwrap();

        // 3. Crear Textura Overlay (Transparente para UI)
        // Debe coincidir en tamaño con la textura base para que el shader funcione
        let overlay_size = (page_bitmap.width * page_bitmap.height * 4) as usize;
        let overlay_data = vec![0u8; overlay_size];
        let overlay_texture = Texture::from_bytes(
            &gpu.device, &gpu.queue, &overlay_data, page_bitmap.width, page_bitmap.height, Some("Overlay")
        ).unwrap();

        // Configuración estándar de WGPU (BindGroups, Pipeline)
        let texture_bind_group_layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
            ],
            label: Some("texture_bind_group_layout"),
        });

        let diffuse_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&diffuse_texture.view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&diffuse_texture.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&overlay_texture.view) },
            ],
            label: Some("diffuse_bind_group"),
        });

        let shader = gpu.device.create_shader_module(wgpu::include_wgsl!("../assets/shaders/shader.wgsl"));
        let render_pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress, shader_location: 1, format: wgpu::VertexFormat::Float32x2 },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, strip_index_format: None, front_face: wgpu::FrontFace::Ccw, cull_mode: Some(wgpu::Face::Back), polygon_mode: wgpu::PolygonMode::Fill, unclipped_depth: false, conservative: false },
            depth_stencil: None,
            multisample: wgpu::MultisampleState { count: 1, mask: !0, alpha_to_coverage_enabled: false },
            multiview: None,
        });

        let vertex_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            gpu,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            diffuse_bind_group,
            document,
            current_page: 0,
            num_indices: INDICES.len() as u32,
        }
    }

    pub fn size(&self) -> winit::dpi::PhysicalSize<u32> {
        self.gpu.size
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gpu.resize(new_size);
    }

    pub fn input(&mut self, _event: &WindowEvent) -> bool {
        false
    }

    pub fn update(&mut self) {}

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.gpu.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.1, g: 0.1, b: 0.15, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

// Función auxiliar para generar un bitmap vacío (negro) si no hay PDF
fn create_fallback_bitmap() -> crate::pdf::render::PageBitmap {
    crate::pdf::render::PageBitmap {
        width: 1,
        height: 1,
        data: vec![0, 0, 0, 255], // 1 pixel negro opaco (BGRA)
    }
}
