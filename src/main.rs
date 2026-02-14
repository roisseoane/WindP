use windp::state::State;
use windp::pdf::PdfSystem; // Necesitamos instanciarlo aquí para manejar lifetimes
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() {
    env_logger::init();

    // 1. Capturar argumentos de lanzamiento (Para "Abrir con...")
    let args: Vec<String> = std::env::args().collect();
    let file_path = if args.len() > 1 {
        // args[0] es el ejecutable, args[1] es el archivo PDF
        Some(args[1].clone())
    } else {
        None
    };

    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new()
        .with_title("WindP")
        .with_inner_size(winit::dpi::PhysicalSize::new(1200, 800))
        .with_transparent(true)
        .with_decorations(true)
        .build(&event_loop)
        .unwrap();

    // 2. Inicializar sistema PDF en el hilo principal
    // Lo creamos aquí para que viva tanto como la ventana
    let pdf_system = PdfSystem::new();

    // 3. Pasamos el sistema y la ruta (si existe) al Estado
    let mut state = pollster::block_on(State::new(&window, &pdf_system, file_path));

    let _ = event_loop.run(move |event, elwt| {
        match event {
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
                                Err(wgpu::SurfaceError::Lost) => state.resize(state.size()),
                                Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                                Err(e) => eprintln!("{:?}", e),
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::AboutToWait => {
                window.request_redraw();
            }
            _ => {}
        }
    });
}
