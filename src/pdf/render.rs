use pdfium_render::prelude::*;
use image::{ImageBuffer, Rgba};

pub struct PageBitmap {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // Bytes crudos BGRA/RGBA listos para la GPU
}

/// Renderiza una página específica a una escala dada.
/// scale_factor: 1.0 = tamaño original (72 DPI), 2.0 = HiDPI/Retina.
pub fn render_page_to_memory(
    document: &PdfDocument,
    page_index: u16,
    scale_factor: f32,
) -> Result<PageBitmap, PdfiumError> {
    // 1. Obtener acceso a la página
    let page = document.pages().get(page_index)?;

    // 2. Calcular dimensiones en píxeles físicos
    let width = (page.width().value * scale_factor) as i32;
    let height = (page.height().value * scale_factor) as i32;

    // 3. Configurar renderizado
    // Usamos BGRA_8888 porque wgpu::TextureFormat::Bgra8Unorm es óptimo en Windows.
    // Flags: LCD_TEXT para subpixel rendering (texto nítido) y NO_SMOOTHPATH para velocidad si fuera necesario,
    // pero aquí priorizamos calidad con defaults + LCD.
    let render_config = PdfRenderConfig::new()
        .set_target_width(width)
        .set_target_height(height)
        .set_format(PdfBitmapFormat::BGRA) 
        .rotate_if_landscape(PdfBitmapRotation::Degrees0, true); // Auto-rotar si es necesario

    // 4. Rasterizar (Operación pesada para la CPU)
    let bitmap = page.render_with_config(&render_config)?;

    // 5. Extraer bytes
    // as_bytes() nos da el buffer crudo sin copias innecesarias.
    let data = bitmap.as_bytes().to_vec();

    Ok(PageBitmap {
        width: width as u32,
        height: height as u32,
        data,
    })
}
