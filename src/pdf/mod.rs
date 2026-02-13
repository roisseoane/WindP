pub mod render;

use pdfium_render::prelude::*;
use std::sync::{Arc, Mutex};

/// Estructura thread-safe que mantiene viva la instancia de PDFium.
/// Usamos Arc para poder compartir referencias entre hilos si decidimos
/// renderizar en background más adelante.
#[derive(Clone)]
pub struct PdfSystem {
    library: Arc<Pdfium>,
}

impl PdfSystem {
    pub fn new() -> Self {
        // Enlazamos dinámicamente con la DLL que descargó build.rs
        // Intentamos cargar localmente primero, luego en sistema.
        let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
            .or_else(|_| Pdfium::bind_to_system_library())
            .expect("CRITICAL: No se pudo cargar pdfium.dll. Asegúrate de que build.rs se ejecutó correctamente.");

        let pdfium = Pdfium::new(bindings);

        Self {
            library: Arc::new(pdfium),
        }
    }

    /// Abre un archivo PDF desde el disco.
    /// Retorna un documento gestionado que limpia su memoria al cerrarse.
    pub fn open_file(&self, path: &str) -> Result<PdfDocument, PdfiumError> {
        self.library.load_pdf_from_file(path, None)
    }
}
