// Estructura de datos que viene de la CPU (State.rs)
struct CameraUniform {
    scale: vec2<f32>,
    translation: vec2<f32>,
}

@group(1) @binding(0) var<uniform> camera: CameraUniform;

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
    
    // Aplicamos la transformación de cámara: Escalar y luego Mover
    let scaled_pos = vec2<f32>(model.position.x * camera.scale.x, model.position.y * camera.scale.y);
    let final_pos = vec4<f32>(scaled_pos + camera.translation, 0.0, 1.0);
    
    out.clip_position = final_pos;
    return out;
}

// Fragment Shader (Igual que antes, pero asegurando el binding correcto)
@group(0) @binding(0) var t_diffuse: texture_2d<f32>;
@group(0) @binding(1) var s_diffuse: sampler;
@group(0) @binding(2) var t_overlay: texture_2d<f32>; 

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let pdf_color = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    let overlay_color = textureSample(t_overlay, s_diffuse, in.tex_coords);

    // Mezcla: El overlay (subrayador) multiplica el color base
    let base_mix = mix(pdf_color, overlay_color * pdf_color, overlay_color.a);
    
    // UI Glassmorphism (Hardcodeado visualmente por ahora)
    let is_bottom_bar = in.clip_position.y > 0.90; // Usar clip_position para UI fija relativa a ventana podría requerir otro paso
    
    return base_mix;
}
