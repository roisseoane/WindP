use anyhow::Context;
use std::env;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

fn main() -> anyhow::Result<()> {
    // Solo ejecutamos esta lógica en Windows, ya que el target es .exe/.msi
    #[cfg(target_os = "windows")]
    {
        setup_pdfium()?;
    }
    
    // Instrucciones para que el linker sepa dónde buscar (aunque sea carga dinámica)
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-search=native={}", manifest_dir);
    
    Ok(())
}

#[cfg(target_os = "windows")]
fn setup_pdfium() -> anyhow::Result<()> {
    // Definimos la URL de la última versión estable para Windows x64
    // Usamos el .zip que es nativo para Windows (evitando dependencias de tar/gz extra)
    const PDFIUM_URL: &str = "https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-win-x64.zip";
    const DLL_NAME: &str = "pdfium.dll";

    // Determinamos dónde dejar la DLL. 
    // La ponemos en la raíz del proyecto para que 'cargo run' la encuentre inmediatamente.
    let root_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let dll_path = root_dir.join(DLL_NAME);

    // Si ya existe, no hacemos nada (ahorramos ancho de banda y tiempo)
    if dll_path.exists() {
        println!("cargo:warning=PDFium DLL ya detectada en: {:?}", dll_path);
        return Ok(());
    }

    println!("cargo:warning=Descargando PDFium desde {}...", PDFIUM_URL);

    // 1. Descargar el ZIP en memoria
    let response = reqwest::blocking::get(PDFIUM_URL)
        .context("Fallo al descargar PDFium")?
        .bytes()
        .context("Fallo al leer bytes del ZIP")?;

    let cursor = Cursor::new(response);
    let mut zip = zip::ZipArchive::new(cursor).context("Fallo al abrir el ZIP")?;

    // 2. Buscar la DLL dentro del ZIP (generalmente está en bin/pdfium.dll)
    // Iteramos para encontrarla sin depender de la ruta exacta de la carpeta interna
    let mut dll_file = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap())
        .find(|f| f.name().ends_with("bin/pdfium.dll") || f.name() == DLL_NAME)
        .context("No se encontró pdfium.dll dentro del ZIP descargado")?;

    // 3. Escribir la DLL en el disco
    let mut out_file = fs::File::create(&dll_path)
        .context(format!("Fallo al crear el archivo {:?}", dll_path))?;
    
    std::io::copy(&mut dll_file, &mut out_file)
        .context("Fallo al extraer/escribir pdfium.dll")?;

    println!("cargo:warning=PDFium instalado correctamente en: {:?}", dll_path);
    Ok(())
}
