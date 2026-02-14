use winit::{
    event::{ElementState, KeyEvent, WindowEvent, MouseScrollDelta, MouseButton},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};
use wgpu::util::DeviceExt;
use crate::gpu::{GpuContext, texture::Texture};
use crate::pdf::{PdfSystem, render::render_page_to_memory};
use crate::ui::{UiState, Tool}; 
use pdfium_render::prelude::*;

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
    ui_flags: [f32; 2], 
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
    
    // BindGroups
    diffuse_bind_group: wgpu::BindGroup,
    camera_bind_group: wgpu::BindGroup,
    
    // Texturas Dinámicas (Para poder actualizarlas)
    diffuse_texture: Texture,
    overlay_texture: Texture,
    overlay_buffer: Vec<u8>, // Copia en CPU para pintar rápido
    page_width: u32,
    page_height: u32,
    
    // Estado Cámara
    camera_buffer: wgpu::Buffer,
    camera_uniform: CameraUniform,
    zoom: f32,
    pan: [f32; 2],
    
    // Lógica App
    ui: UiState,
    document: Option<PdfDocument<'a>>,
    current_page: u16,
    total_pages: u16,
    
    // Input State
    mouse_pressed: bool,
    last_mouse_pos: [f64; 2], // Para calcular el delta del drag
    
    num_indices: u32,
}

impl<'a> State<'a> {
    pub async fn new(window: &Window, pdf_system: &'a PdfSystem, file_path: Option<String>) -> Self {
        let gpu = GpuContext::new(window).await;
        let ui = UiState::new(&gpu.device, &gpu.queue);

        // 1. Cargar PDF Inicial
        let (document, page_bitmap, total) = if let Some(path) = file_path {
            match pdf_system.open_file(&path) {
                Ok(doc) => {
                    let total = doc.pages().len();
                    let bitmap = render_page_to_memory(&doc, 0, 1.5).unwrap_or_else(|_| create_fallback());
                    (Some(doc), bitmap, total)
                },
                Err(_) => (None, create_fallback(), 0)
            }
        } else {
             (None, create_fallback(), 0)
        };

        // 2. Crear Texturas
        let diffuse_texture = Texture::from_bytes(&gpu.device, &gpu.queue, &page_bitmap.data, page_bitmap.width, page_bitmap.height, Some("PDF")).unwrap();
        
        // Overlay (Buffer negro transparente)
        let overlay_size = (page_bitmap.width * page_bitmap.height * 4) as usize;
        let overlay_buffer = vec![0u8; overlay_size];
        let overlay_texture = Texture::from_bytes(&gpu.device, &gpu.queue, &overlay_buffer, page_bitmap.width, page_bitmap.height, Some("Overlay")).unwrap();

        // 3. Pipeline Config
        let texture_bg_layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { multisampled: false, view_dimension: wgpu::TextureViewDimension::D2, sample_type: wgpu::TextureSampleType::Float { filterable: true } }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { multisampled: false, view_dimension: wgpu::TextureViewDimension::D2, sample_type: wgpu::TextureSampleType::Float { filterable: true } }, count: None },
            ],
            label: Some("Texture BG Layout"),
        });

        let diffuse_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bg_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&diffuse_texture.view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&diffuse_texture.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&overlay_texture.view) },
            ],
            label: Some("Diffuse BG"),
        });

        let camera_uniform = CameraUniform { scale: [1.0, 1.0], translation: [0.0, 0.0], ui_flags: [0.0, 0.0] };
        let camera_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bg_layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
            label: Some("Camera BG Layout"),
        });

        let camera_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bg_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() }],
            label: Some("Camera BG"),
        });

        let shader = gpu.device.create_shader_module(wgpu::include_wgsl!("../assets/shaders/shader.wgsl"));
        let render_pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&texture_bg_layout, &camera_bg_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: "vs_main", buffers: &[wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress, step_mode: wgpu::VertexStepMode::Vertex, attributes: &[wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 }, wgpu::VertexAttribute { offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress, shader_location: 1, format: wgpu::VertexFormat::Float32x2 }] }] },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_main", targets: &[Some(wgpu::ColorTargetState { format: gpu.config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }),
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
            gpu, render_pipeline, vertex_buffer, index_buffer,
            diffuse_bind_group, camera_bind_group, camera_buffer, camera_uniform,
            diffuse_texture, overlay_texture, overlay_buffer,
            page_width: page_bitmap.width, page_height: page_bitmap.height,
            zoom: 1.0, pan: [0.0, 0.0],
            ui, document, current_page: 0, total_pages: total,
            mouse_pressed: false, last_mouse_pos: [0.0, 0.0],
            num_indices: INDICES.len() as u32,
        }
    }

    // --- LÓGICA CORE ---

    fn load_page(&mut self, page_idx: u16) {
        if let Some(doc) = &self.document {
            if let Ok(bitmap) = render_page_to_memory(doc, page_idx, 1.5) {
                // 1. Actualizar Textura del PDF
                self.gpu.queue.write_texture(
                    wgpu::ImageCopyTexture { texture: &self.diffuse_texture.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    &bitmap.data,
                    wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * bitmap.width), rows_per_image: Some(bitmap.height) },
                    wgpu::Extent3d { width: bitmap.width, height: bitmap.height, depth_or_array_layers: 1 }
                );
                
                // 2. Limpiar Overlay (Subrayados)
                self.overlay_buffer.fill(0);
                self.gpu.queue.write_texture(
                    wgpu::ImageCopyTexture { texture: &self.overlay_texture.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    &self.overlay_buffer,
                    wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * bitmap.width), rows_per_image: Some(bitmap.height) },
                    wgpu::Extent3d { width: bitmap.width, height: bitmap.height, depth_or_array_layers: 1 }
                );

                self.current_page = page_idx;
                self.page_width = bitmap.width;
                self.page_height = bitmap.height;
                println!("Página cargada: {}", page_idx + 1);
            }
        }
    }

    fn paint_overlay(&mut self, ndc_x: f64, ndc_y: f64) {
        // Transformar NDC (-1 a 1) a Espacio Textura (0 a Width)
        // Invertimos la transformación de cámara: (ndc - translation) / scale
        let aspect = self.gpu.size.width as f32 / self.gpu.size.height as f32;
        
        let x_cam = (ndc_x as f32 - self.pan[0]) / self.zoom;
        let y_cam = (ndc_y as f32 - self.pan[1]) / (self.zoom * aspect); // Corregir por aspect ratio vertical si se aplica en shader? 
        // Nota: En shader usamos scale.y = zoom * aspect. Revisar shader.wgsl vs update()
        // En update: scale.y = zoom * aspect. Entonces Y_cam = (y - pan.y) / scale.y.
        
        // Coordenadas UV (0 a 1)
        // El quad es de -1 a 1. UV 0,0 es TopLeft.
        let u = (x_cam + 1.0) * 0.5;
        let v = (1.0 - y_cam) * 0.5;

        if u >= 0.0 && u <= 1.0 && v >= 0.0 && v <= 1.0 {
            let tx = (u * self.page_width as f32) as i32;
            let ty = (v * self.page_height as f32) as i32;
            let radius = 5; // Radio del pincel
            
            let mut modified = false;

            // Dibujar círculo simple en el buffer CPU
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx*dx + dy*dy <= radius*radius {
                        let px = tx + dx;
                        let py = ty + dy;
                        if px >= 0 && px < self.page_width as i32 && py >= 0 && py < self.page_height as i32 {
                            let idx = ((py as u32 * self.page_width + px as u32) * 4) as usize;
                            // Amarillo fluorescente (RGBA)
                            self.overlay_buffer[idx] = 255;   // R
                            self.overlay_buffer[idx+1] = 255; // G
                            self.overlay_buffer[idx+2] = 0;   // B
                            self.overlay_buffer[idx+3] = 100; // Alpha (Semi-transparente)
                            modified = true;
                        }
                    }
                }
            }

            if modified {
                // Subir TODO el buffer a la GPU (Optimización futura: subir solo región sucia)
                self.gpu.queue.write_texture(
                    wgpu::ImageCopyTexture { texture: &self.overlay_texture.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    &self.overlay_buffer,
                    wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * self.page_width), rows_per_image: Some(self.page_height) },
                    wgpu::Extent3d { width: self.page_width, height: self.page_height, depth_or_array_layers: 1 }
                );
            }
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gpu.resize(new_size);
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                let pressed = *state == ElementState::Pressed;
                self.mouse_pressed = pressed;
                
                if pressed {
                    // 1. Chequear UI
                    if self.ui.hit_test(self.last_mouse_pos[0], self.last_mouse_pos[1], self.gpu.size.width as f64, self.gpu.size.height as f64) {
                        return true; 
                    }
                }
                true
            },
            WindowEvent::CursorMoved { position, .. } => {
                // Normalizado -1 a 1
                let x = (position.x / self.gpu.size.width as f64) * 2.0 - 1.0;
                let y = -((position.y / self.gpu.size.height as f64) * 2.0 - 1.0);
                
                let dx = x - self.last_mouse_pos[0];
                let dy = y - self.last_mouse_pos[1];
                self.last_mouse_pos = [x, y];

                if self.mouse_pressed {
                    match self.ui.active_tool {
                        Tool::Pan => {
                            // Arrastrar documento
                            self.pan[0] += dx as f32;
                            self.pan[1] += dy as f32;
                        },
                        Tool::Highlighter => {
                            // Pintar
                            self.paint_overlay(x, y);
                        },
                        _ => {}
                    }
                }
                true
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta { MouseScrollDelta::LineDelta(_, y) => *y * 0.1, MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.001 };
                self.zoom = (self.zoom + scroll).clamp(0.1, 10.0);
                true
            },
            WindowEvent::KeyboardInput { event: KeyEvent { state: ElementState::Pressed, physical_key: PhysicalKey::Code(keycode), .. }, .. } => {
                match keycode {
                    KeyCode::ArrowRight => {
                        if self.current_page < self.total_pages - 1 {
                            self.load_page(self.current_page + 1);
                        }
                        true
                    },
                    KeyCode::ArrowLeft => {
                        if self.current_page > 0 {
                            self.load_page(self.current_page - 1);
                        }
                        true
                    },
                    _ => false,
                }
            },
            _ => false,
        }
    }

    pub fn update(&mut self) {
        // Mantener el aspect ratio correcto del PDF
        let aspect = self.gpu.size.width as f32 / self.gpu.size.height as f32;
        self.camera_uniform.scale = [self.zoom, self.zoom * aspect]; 
        self.camera_uniform.translation = self.pan;
        self.camera_uniform.ui_flags[0] = if self.ui.is_carousel_open { 1.0 } else { 0.0 };
        self.gpu.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[self.camera_uniform]));
    }

    pub fn size(&self) -> winit::dpi::PhysicalSize<u32> { self.gpu.size }

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
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.05, g: 0.05, b: 0.08, a: 1.0 }), store: wgpu::StoreOp::Store },
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

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

fn create_fallback() -> crate::pdf::render::PageBitmap {
    crate::pdf::render::PageBitmap { width: 1, height: 1, data: vec![0, 0, 0, 255] }
}
