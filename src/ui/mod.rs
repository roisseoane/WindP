pub mod assets;
use wgpu::util::DeviceExt;
use crate::gpu::texture::Texture;

pub enum Tool {
    None,
    Pan,
    Highlighter,
}

pub struct UiState {
    pub active_tool: Tool,
    pub is_carousel_open: bool,
    
    // Texturas de Iconos
    pub icon_search: Texture,
    pub icon_pen: Texture,
    pub icon_menu: Texture,
    
    // Layout (Hardcodeado por eficiencia extrema)
    pub bottom_bar_height: f32,
    pub side_panel_width: f32,
}

impl UiState {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        // Generar iconos procedurles
        let size = 64;
        let search_bytes = assets::IconGenerator::generate_search_icon(size);
        let pen_bytes = assets::IconGenerator::generate_pen_icon(size);
        let menu_bytes = assets::IconGenerator::generate_menu_icon(size);

        let icon_search = Texture::from_bytes(device, queue, &search_bytes, size, size, Some("Icon Search")).unwrap();
        let icon_pen = Texture::from_bytes(device, queue, &pen_bytes, size, size, Some("Icon Pen")).unwrap();
        let icon_menu = Texture::from_bytes(device, queue, &menu_bytes, size, size, Some("Icon Menu")).unwrap();

        Self {
            active_tool: Tool::Pan,
            is_carousel_open: false,
            icon_search,
            icon_pen,
            icon_menu,
            bottom_bar_height: 80.0,
            side_panel_width: 200.0,
        }
    }

    // Detectar clicks en la UI
    // Retorna true si el click fue en la UI (para no mover el PDF)
    pub fn hit_test(&mut self, x: f64, y: f64, win_width: f64, win_height: f64) -> bool {
        // Coordenadas x,y vienen normalizadas de -1 a 1 (sistema WGPU)
        // Convertimos a píxeles pantalla para facilitar lógica UI
        let px = (x + 1.0) * 0.5 * win_width;
        let py = (1.0 - y) * 0.5 * win_height; // Invertimos Y para que 0 sea arriba

        // 1. Chequear Barra Inferior
        if py > (win_height - self.bottom_bar_height as f64) {
            // Zona de botones (Centro)
            let center = win_width / 2.0;
            
            // Botón Menú (Carrusel)
            if px > center - 100.0 && px < center - 60.0 {
                self.is_carousel_open = !self.is_carousel_open;
                println!("UI: Toggle Carrusel");
                return true;
            }
            // Botón Lápiz
            if px > center - 20.0 && px < center + 20.0 {
                self.active_tool = match self.active_tool {
                    Tool::Highlighter => Tool::Pan,
                    _ => Tool::Highlighter,
                };
                println!("UI: Herramienta Lápiz {:?}", match self.active_tool { Tool::Highlighter => "ON", _ => "OFF"});
                return true;
            }
            // Botón Buscar (Dummy)
            if px > center + 60.0 && px < center + 100.0 {
                println!("UI: Buscar (Ctrl+F simulado)");
                return true;
            }
            return true; // Click en la barra, aunque no sea botón
        }

        // 2. Chequear Panel Lateral (si está abierto)
        if self.is_carousel_open && px < self.side_panel_width as f64 {
            println!("UI: Click en Carrusel");
            return true;
        }

        false
    }
}
