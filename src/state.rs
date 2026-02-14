use winit::{
    event::{ElementState, KeyEvent, WindowEvent, MouseScrollDelta, MouseButton},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};
use wgpu::util::DeviceExt;
use crate::gpu::{GpuContext, texture::Texture};
use crate::pdf::{PdfSystem, render::render_page_to_memory};
use pdfium_render::prelude::*;

// --- Estructuras de Datos ---

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    scale: [f32; 2],
    translation: [f32; 2],
}

const VERTICES: &[Vertex] = &[
    Vertex { position: [-1.0, 1.0, 0.0], tex_coords: [0.0, 0.0] },
    Vertex { position: [-1.0, -1.0, 0.0], tex_coords: [0.0, 1.0] },
    Vertex { position: 1.0, -1.0, 0.0], tex_coords: [1.0, 1.0] },
    Vertex { position: 1.0, 1.0, 0.0], tex_coords: [1.0, 0.0] },
];

const INDICES: &[u16] = &[0, 1, 2, 2, 3, 0];

// --- Estado Principal ---

pub struct State<'a> {
    gpu: GpuContext,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    diffuse_bind_group: wgpu::BindGroup,
    
    // Cámara / Transformación
    camera_bind_group: wgpu::BindGroup,
    camera_buffer: wgpu::Buffer,
    camera_uniform: CameraUniform,
    zoom: f32,
    pan: [f32; 2],

    // Estado Lógico PDF
    document: Option<PdfDocument<'a>>,
    current_page: u16,
    total_pages: u16,
    
    // Interacción
    mouse_pressed: bool,
    mouse_pos: [f64; 2], // Posición normalizada (-1 a 1)
    
    num_indices: u32,
}

impl<'a> State<'a> {
    pub async fn new(
        window: &Window, 
        pdf_system: &'a PdfSystem, 
        file_path: Option<String>
    ) -> Self {
        let gpu = GpuContext::new(window).await;
        
        // 1. Carga Inicial del PDF
        let (document, page_bitmap, total_pages) = if let Some(path) = file_path {
            match pdf_system.open_file(&path) {
                Ok(doc) => {
                    let total = doc.pages().len();
                    let bitmap = render_page_to_memory(&doc, 0, 1.5).unwrap_or_else(|_| create_fallback_bitmap());
                    (Some(doc), bitmap, total)
                },
                Err(e) => {
                    eprintln!("Error: {}", e);
                    (None, create_fallback_bitmap(), 0)
                }
            }
        } else {
            (None, create_fallback_bitmap(), 0)
        };

        // 2. Texturas
        let diffuse_texture = Texture::from_bytes(&gpu.device, &gpu.queue, &page_bitmap.data, page_bitmap.width, page_bitmap.height, Some("PDF")).unwrap();
        
        let overlay_size = (page_bitmap.width * page_bitmap.height * 4) as usize;
        let overlay_data = vec![0u8; overlay_size];
        let overlay_texture = Texture::from_bytes(&gpu.device, &gpu.queue, &overlay_data, page_bitmap.width, page_bitmap.height, Some("Overlay")).unwrap();

        // 3. Bind Groups (Texturas)
        let texture_bind_group_layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { multisampled: false, view_dimension: wgpu::TextureViewDimension::D2, sample_type: wgpu::TextureSampleType::Float { filterable: true } }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { multisampled: false, view_dimension: wgpu::TextureViewDimension::D2, sample_type: wgpu::TextureSampleType::Float { filterable: true } }, count: None },
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

        // 4. Cámara (Uniforms)
        let camera_uniform = CameraUniform { scale: [1.0, 1.0], translation: [0.0, 0.0] };
        let camera_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
            label: Some("camera_bind_group_layout"),
        });

        let camera_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() }],
            label: Some("camera_bind_group"),
        });

        // 5. Pipeline
        let shader = gpu.device.create_shader_module(wgpu::include_wgsl!("../assets/shaders/shader.wgsl"));
        let render_pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&texture_bind_group_layout, &camera_bind_group_layout], // ¡Importante: 2 grupos!
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
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
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
            camera_bind_group,
            camera_buffer,
            camera_uniform,
            zoom: 1.0,
            pan: [0.0, 0.0],
            document,
            current_page: 0,
            total_pages,
            mouse_pressed: false,
            mouse_pos: [0.0, 0.0],
            num_indices: INDICES.len() as u32,
        }
    }

    pub fn size(&self) -> winit::dpi::PhysicalSize<u32> { self.gpu.size }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gpu.resize(new_size);
        // Aquí deberíamos recalcular el aspect ratio
    }

    // --- LÓGICA DE INTERACCIÓN (Input Handling) ---
    pub fn input(&mut self, event: &WindowEvent) -> bool {
        match event {
            // Teclado: Flechas para cambiar página
            WindowEvent::KeyboardInput {
                event: KeyEvent { state: ElementState::Pressed, physical_key: PhysicalKey::Code(keycode), .. }, ..
            } => {
                match keycode {
                    KeyCode::ArrowRight => {
                        // Lógica futura: Cambiar página
                        println!("Acción: Siguiente página (Pendiente de renderizar nueva textura)");
                        true
                    },
                    KeyCode::ArrowLeft => {
                         println!("Acción: Página anterior");
                         true
                    },
                    _ => false,
                }
            },
            // Ratón: Rueda para Zoom/Pan Vertical
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y * 0.1,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.001,
                };
                // Si Control está presionado: Zoom, si no: Pan Vertical
                // Por simplicidad, aquí hacemos Zoom simple
                let old_zoom = self.zoom;
                self.zoom = (self.zoom + scroll).clamp(0.1, 5.0);
                
                // Ajustar pan para hacer zoom hacia el centro (simplificado)
                true
            },
            // Ratón: Click para pintar (Subrayador)
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                self.mouse_pressed = *state == ElementState::Pressed;
                true
            },
            // Ratón: Movimiento
            WindowEvent::CursorMoved { position, .. } => {
                // Normalizar coordenadas a (-1.0 a 1.0)
                let x = (position.x / self.gpu.size.width as f64) * 2.0 - 1.0;
                let y = -((position.y / self.gpu.size.height as f64) * 2.0 - 1.0);
                self.mouse_pos = [x, y];
                
                if self.mouse_pressed {
                    // AQUÍ IRÍA LA LÓGICA DE PINTAR EN LA TEXTURA DE OVERLAY
                    // Se requiere mapear coordenada de pantalla a coordenada de textura
                    // y usar self.gpu.queue.write_texture para actualizar pixeles.
                }
                true
            },
            _ => false,
        }
    }

    pub fn update(&mut self) {
        // Actualizar Uniforms en GPU
        self.camera_uniform.scale = [self.zoom, self.zoom * (self.gpu.size.width as f32 / self.gpu.size.height as f32)]; // Aspect ratio correction básica
        self.camera_uniform.translation = self.pan;
        
        self.gpu.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[self.camera_uniform]));
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.gpu.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.05, g: 0.05, b: 0.05, a: 1.0 }), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            // Grupo 0: Texturas (PDF + Overlay)
            render_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
            // Grupo 1: Cámara (Posición/Zoom)
            render_pass.set_bind_group(1, &self.camera_bind_group, &[]);
            
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

fn create_fallback_bitmap() -> crate::pdf::render::PageBitmap {
    crate::pdf::render::PageBitmap { width: 1, height: 1, data: vec![0, 0, 0, 255] }
}
