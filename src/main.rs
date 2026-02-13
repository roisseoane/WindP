use windp::state::State; // Asumimos que state.rs expondrá la lógica principal
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() {
    // 1. Inicializar logger para debug (coste cero en release)
    env_logger::init();

    // 2. Crear el bucle de eventos del sistema operativo
    let event_loop = EventLoop::new().unwrap();

    // 3. Configurar la ventana nativa
    let window = WindowBuilder::new()
        .with_title("WindP - Visualizador de Alto Rendimiento")
        .with_inner_size(winit::dpi::PhysicalSize::new(1200, 800))
        .with_transparent(true) // Crucial para glassmorphism (depende del compositor del OS)
        .with_decorations(true) // Mantenemos bordes por ahora, luego podemos personalizarlos
        .build(&event_loop)
        .unwrap();

    // 4. Inicializar el Estado de la App (GPU + Lógica)
    // Usamos pollster para bloquear el hilo main solo durante la carga inicial
    // ya que wgpu es asíncrono por naturaleza.
    let mut state = pollster::block_on(State::new(&window));

    // 5. Arrancar el bucle infinito
    let _ = event_loop.run(move |event, elwt| {
        match event {
            // Evento: La ventana pide redibujarse
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == window.id() => {
                if !state.input(event) {
                    match event {
                        WindowEvent::CloseRequested
                        | WindowEvent::KeyboardInput {
                            event:
                                KeyEvent {
                                    state: ElementState::Pressed,
                                    physical_key: winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape),
                                    ..
                                },
                            ..
                        } => elwt.exit(),
                        
                        WindowEvent::Resized(physical_size) => {
                            state.resize(*physical_size);
                        }
                        
                        WindowEvent::RedrawRequested => {
                            state.update();
                            match state.render() {
                                Ok(_) => {}
                                // Si perdemos la superficie (ej: minimizar), la reconfiguramos
                                Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                                // Si falta memoria, salimos (mejor crashear que corromper)
                                Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                                Err(e) => eprintln!("{:?}", e),
                            }
                        }
                        _ => {}
                    }
                }
            }
            // Evento: La CPU está ociosa, pedimos redibujar para mantener FPS estables
            Event::AboutToWait => {
                window.request_redraw();
            }
            _ => {}
        }
    });
}
