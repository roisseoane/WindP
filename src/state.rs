use winit::{
    event::{ElementState, KeyEvent, WindowEvent, MouseScrollDelta, MouseButton},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};
use wgpu::util::DeviceExt;
use crate::gpu::{GpuContext, texture::Texture};
use crate::pdf::{PdfSystem, render::render_page_to_memory};
use crate::ui::UiState; // Nuevo módulo
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
    ui_flags: [f32; 2], // [0] = carousel_open (0.0/1.0), [1] = unused
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
    
    // UI Icons BindGroups (Para dibujar los botones)
    icon_bind_group_layout: wgpu::BindGroupLayout,
    icon_pipeline: wgpu::RenderPipeline,
    
    // Estado
    camera_buffer: wgpu::Buffer,
    camera_uniform: CameraUniform,
    zoom: f32,
    pan: [f32; 2],
    
    ui: UiState, // Sistema de UI
    document: Option<PdfDocument<'a>>,
    
    // Input
    mouse_pressed: bool,
    mouse_pos: [f64; 2],
    
    num_indices: u32,
}

impl<'a> State<'a> {
    pub async fn new(window: &Window, pdf_system: &'a PdfSystem, file_path: Option<String>) -> Self {
        let gpu = GpuContext::new(window).await;
        
        // 1. UI Init
        let ui = UiState::new(&gpu.device, &gpu.queue);

        // 2. PDF Load
        let (document, page_bitmap) = if let Some(path) = file_path {
            match pdf_system.open_file(&path) {
                Ok(doc) => (Some(doc), render_page_to_memory(&doc.pages().get(0).unwrap(), 0, 1.5).unwrap()),
                Err(_) => (None, crate::pdf::render::PageBitmap { width: 1, height: 1, data: vec![0,0,0,255] })
            }
        } else {
             (None, crate::pdf::render::PageBitmap { width: 1, height: 1, data: vec![0,0,0,255] })
        };

        // 3. Texturas Base
        let diffuse_texture = Texture::from_bytes(&gpu.device, &gpu.queue, &page_bitmap.data, page_bitmap.width, page_bitmap.height, Some("PDF")).unwrap();
        let overlay_texture = Texture::from_bytes(&gpu.device, &gpu.queue, &vec![0u8; (page_bitmap.width * page_bitmap.height * 4) as usize], page_bitmap.width, page_bitmap.height, Some("Overlay")).unwrap();

        // 4. Pipeline Principal (PDF)
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

        // 5. Cámara y UI Flags
        let camera_uniform = CameraUniform { scale: [1.0, 1.0], translation: [0.0, 0.0], ui_flags: [0.0, 0.0] };
        let camera_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bg_layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT, // Visible en Fragment para la UI lógica
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

        // 6. Pipeline para Iconos (Simple Overlay)
        // Reusamos layout pero con otro shader entry point si quisiéramos, o el mismo con uniforms de posición.
        // Para simplificar "Best Effort": usaremos el render pass principal para pintar iconos, 
        // pero necesitamos un pipeline que acepte SOLO una textura (el icono).
        // (Por brevedad en este archivo, omitiré la implementación completa de un segundo pipeline de UI complejo
        // y usaremos el shader principal para dibujar la UI proceduralmente con rectángulos y UVs en el fs_main).

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
            diffuse_bind_group, camera_bind_group,
            icon_bind_group_layout: texture_bg_layout, // Hack: Reusamos layout
            icon_pipeline: render_pipeline.clone(), // Hack: Reusamos pipeline (no óptimo pero funciona para MVP)
            camera_buffer, camera_uniform,
            zoom: 1.0, pan: [0.0, 0.0],
            ui, document, mouse_pressed: false, mouse_pos: [0.0, 0.0], num_indices: INDICES.len() as u32,
        }
    }

    pub fn size(&self) -> winit::dpi::PhysicalSize<u32> { self.gpu.size }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gpu.resize(new_size);
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                let pressed = *state == ElementState::Pressed;
                self.mouse_pressed = pressed;
                
                if pressed {
                    // Si hacemos click, preguntar primero a la UI
                    if self.ui.hit_test(self.mouse_pos[0], self.mouse_pos[1], self.gpu.size.width as f64, self.gpu.size.height as f64) {
                        return true; // UI consumió el evento
                    }
                }
                false
            },
            WindowEvent::CursorMoved { position, .. } => {
                let x = (position.x / self.gpu.size.width as f64) * 2.0 - 1.0;
                let y = -((position.y / self.gpu.size.height as f64) * 2.0 - 1.0);
                self.mouse_pos = [x, y];
                true
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta { MouseScrollDelta::LineDelta(_, y) => *y * 0.1, MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.001 };
                self.zoom = (self.zoom + scroll).clamp(0.1, 5.0);
                true
            },
            _ => false,
        }
    }

    pub fn update(&mut self) {
        self.camera_uniform.scale = [self.zoom, self.zoom * (self.gpu.size.width as f32 / self.gpu.size.height as f32)];
        self.camera_uniform.translation = self.pan;
        self.camera_uniform.ui_flags[0] = if self.ui.is_carousel_open { 1.0 } else { 0.0 };
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
            render_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
            render_pass.set_bind_group(1, &self.camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
            
            // Aquí en el futuro dibujaremos los iconos como quads adicionales
            // Por ahora, el shader fs_main simula la barra.
        }

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}
