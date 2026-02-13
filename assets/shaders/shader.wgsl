// Vertex Shader
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coords = model.tex_coords;
    out.clip_position = vec4<f32>(model.position, 1.0);
    return out;
}

// Fragment Shader
@group(0) @binding(0) var t_diffuse: texture_2d<f32>;
@group(0) @binding(1) var s_diffuse: sampler;
@group(0) @binding(2) var t_overlay: texture_2d<f32>; // Capa de subrayados/UI

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // 1. Muestrear el PDF base
    let pdf_color = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    
    // 2. Muestrear la capa de superposición (Subrayados)
    let overlay_color = textureSample(t_overlay, s_diffuse, in.tex_coords);

    // Mezcla básica de subrayado (Multiplicación para efecto marcador)
    let base_mix = mix(pdf_color, overlay_color, overlay_color.a);

    // 3. Lógica de Glassmorphism (Barra inferior y lateral)
    // Definimos zonas de UI basadas en coordenadas (0.0 a 1.0)
    let is_bottom_bar = in.tex_coords.y > 0.90; 
    let is_side_panel = in.tex_coords.x < 0.00; // Oculto por defecto

    if (is_bottom_bar) {
        // Efecto vidrio: Aclarar y desenfocar (simulado con ruido blanco sutil)
        let noise = fract(sin(dot(in.tex_coords, vec2<f32>(12.9898, 78.233))) * 43758.5453);
        let glass_color = vec4<f32>(0.9, 0.9, 0.95, 0.3); // Blanco azulado transparente
        
        return mix(base_mix, glass_color, 0.4) + (noise * 0.02);
    }

    return base_mix;
}
