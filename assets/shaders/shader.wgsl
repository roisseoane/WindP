struct CameraUniform {
    scale: vec2<f32>,
    translation: vec2<f32>,
    ui_flags: vec2<f32>, // x = carousel_open
}

@group(1) @binding(0) var<uniform> camera: CameraUniform;

// ... (Vertex shader igual que antes) ...

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) uv_screen: vec2<f32>, // Coordenadas de pantalla 0..1
}

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coords = model.tex_coords;
    out.uv_screen = vec2<f32>(model.tex_coords.x, 1.0 - model.tex_coords.y); // Corrección Y
    
    let scaled_pos = vec2<f32>(model.position.x * camera.scale.x, model.position.y * camera.scale.y);
    let final_pos = vec4<f32>(scaled_pos + camera.translation, 0.0, 1.0);
    out.clip_position = final_pos;
    return out;
}

@group(0) @binding(0) var t_diffuse: texture_2d<f32>;
@group(0) @binding(1) var s_diffuse: sampler;
@group(0) @binding(2) var t_overlay: texture_2d<f32>;

// Función auxiliar para dibujar círculos (Botones)
fn sdf_circle(uv: vec2<f32>, center: vec2<f32>, radius: f32) -> f32 {
    let d = distance(uv, center);
    return 1.0 - smoothstep(radius, radius + 0.01, d);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // 1. Renderizado PDF + Overlay
    let pdf = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    let ovr = textureSample(t_overlay, s_diffuse, in.tex_coords);
    var color = mix(pdf, ovr * pdf, ovr.a);

    // 2. UI - Coordenadas de pantalla crudas (0,0 a 1,1)
    // Para glassmorphism, necesitamos coordenadas absolutas de ventana, 
    // pero aquí usamos tex_coords del quad fullscreen como aproximación.
    let uv = in.tex_coords; 
    
    // -- Barra Inferior --
    if (uv.y > 0.88) {
        // Fondo Glassmorphism
        let noise = fract(sin(dot(uv, vec2<f32>(12.9, 78.2))) * 43758.5);
        let glass = vec4<f32>(0.1, 0.1, 0.15, 0.85); // Oscuro moderno
        color = mix(color, glass, 0.9) + (noise * 0.01);
        
        // Botones (Procedurales)
        let aspect = camera.scale.x / camera.scale.y; // Aproximación aspect ratio
        
        // Botón Menú (Izquierda)
        let btn_menu = sdf_circle(uv, vec2<f32>(0.4, 0.94), 0.03);
        color = mix(color, vec4<f32>(1.0, 1.0, 1.0, 0.5), btn_menu * 0.3); // Halo

        // Botón Lápiz (Centro)
        let btn_pen = sdf_circle(uv, vec2<f32>(0.5, 0.94), 0.035);
        color = mix(color, vec4<f32>(1.0, 0.6, 0.2, 1.0), btn_pen); // Naranja
        
        // Botón Buscar (Derecha)
        let btn_search = sdf_circle(uv, vec2<f32>(0.6, 0.94), 0.03);
        color = mix(color, vec4<f32>(0.2, 0.6, 1.0, 1.0), btn_search); // Azul
    }

    // -- Panel Lateral (Carrusel) --
    if (camera.ui_flags.x > 0.5 && uv.x < 0.2) {
        let glass_side = vec4<f32>(0.05, 0.05, 0.05, 0.95);
        color = mix(color, glass_side, 0.95);
        
        // Simular miniaturas (cajas grises)
        let thumbnail_y = fract(uv.y * 5.0);
        if (thumbnail_y > 0.1 && thumbnail_y < 0.9 && uv.x > 0.02 && uv.x < 0.18) {
             color = mix(color, vec4<f32>(1.0, 1.0, 1.0, 0.1), 0.5);
        }
    }

    return color;
}
