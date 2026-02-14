pub struct IconGenerator;

impl IconGenerator {
    pub fn generate_search_icon(size: u32) -> Vec<u8> {
        let mut data = vec![0u8; (size * size * 4) as usize];
        let center = size as f32 / 2.0;
        let radius = size as f32 * 0.35;
        
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 - center;
                let dy = y as f32 - center;
                let dist = (dx*dx + dy*dy).sqrt();
                let idx = ((y * size + x) * 4) as usize;
                
                // Círculo de la lupa
                if (dist - radius).abs() < 2.0 {
                    data[idx] = 255; data[idx+1] = 255; data[idx+2] = 255; data[idx+3] = 255;
                } 
                // Mango de la lupa
                else if x as f32 > center + radius * 0.5 && y as f32 > center + radius * 0.5 && (x as f32 - y as f32).abs() < 3.0 {
                     data[idx] = 255; data[idx+1] = 255; data[idx+2] = 255; data[idx+3] = 255;
                }
            }
        }
        data
    }

    pub fn generate_pen_icon(size: u32) -> Vec<u8> {
        let mut data = vec![0u8; (size * size * 4) as usize];
        for y in 0..size {
            for x in 0..size {
                let idx = ((y * size + x) * 4) as usize;
                // Dibujar una línea diagonal (lápiz)
                if (x as f32 + y as f32 - size as f32).abs() < 4.0 && x > size/4 && x < size*3/4 {
                    data[idx] = 255; data[idx+1] = 200; data[idx+2] = 100; data[idx+3] = 255; // Color Naranja
                }
            }
        }
        data
    }

    pub fn generate_menu_icon(size: u32) -> Vec<u8> {
        let mut data = vec![0u8; (size * size * 4) as usize];
        for y in 0..size {
            for x in 0..size {
                let idx = ((y * size + x) * 4) as usize;
                // Tres líneas horizontales
                if (y > size/4 && y < size/4 + 4) || (y > size/2 && y < size/2 + 4) || (y > size*3/4 && y < size*3/4 + 4) {
                     if x > size/4 && x < size*3/4 {
                        data[idx] = 255; data[idx+1] = 255; data[idx+2] = 255; data[idx+3] = 255;
                     }
                }
            }
        }
        data
    }
}
