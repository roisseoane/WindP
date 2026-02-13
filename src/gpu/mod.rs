use winit::window::Window;

pub struct GpuContext {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
}

impl GpuContext {
    // Constructor asíncrono: Configura el pipeline gráfico
    pub async fn new(window: &Window) -> Self {
        let size = window.inner_size();

        // 1. Instancia: El punto de entrada a wgpu
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // 2. Superficie: Donde vamos a dibujar en la ventana
        // Usamos unsafe porque la superficie debe sobrevivir al contexto,
        // pero en este diseño el ciclo de vida está atado a la ventana.
        let surface = unsafe { instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::from_window(&window).unwrap()) }.unwrap();

        // 3. Adaptador: Handle a la tarjeta gráfica física
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance, // ¡Prioridad Rendimiento!
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).await.unwrap();

        // 4. Dispositivo (Lógico) y Cola de Comandos
        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                label: None,
            },
            None, // Trace path
        ).await.unwrap();

        // 5. Configuración del SwapChain
        let surface_caps = surface.get_capabilities(&adapter);
        // Preferimos sRGB para colores precisos
        let surface_format = surface_caps.formats.iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo, // VSync activado (evita tearing)
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        Self {
            surface,
            device,
            queue,
            config,
            size,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }
}
