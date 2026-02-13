use winit::{event::WindowEvent, window::Window};
use crate::gpu::GpuContext;

pub struct State {
    gpu: GpuContext,
    // Aquí añadiremos más adelante:
    // pub pdf_renderer: PdfRenderer,
    // pub ui_overlay: UiOverlay,
}

impl State {
    pub async fn new(window: &Window) -> Self {
        let gpu = GpuContext::new(window).await;
        
        Self {
            gpu,
        }
    }

    pub fn size(&self) -> winit::dpi::PhysicalSize<u32> {
        self.gpu.size
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gpu.resize(new_size);
    }

    // Devuelve true si el evento fue consumido (ej: click en botón UI)
    pub fn input(&mut self, _event: &WindowEvent) -> bool {
        false 
    }

    pub fn update(&mut self) {
        // Aquí actualizaremos las animaciones del glassmorphism
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        // Obtener la siguiente textura del swap chain
        let output = self.gpu.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        {
            // Iniciar pase de renderizado (limpiar pantalla con un color base)
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1, // Fondo oscuro moderno
                            g: 0.1,
                            b: 0.12,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
        }

        // Enviar comandos a la GPU
        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
