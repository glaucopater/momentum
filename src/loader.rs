use anyhow::{anyhow, Result};
use image::{DynamicImage, ImageBuffer, Rgb};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::io::Cursor;
use exif::{Reader, Tag, In, Value};

#[derive(Debug)]
pub struct LoadedImage {
    pub image: DynamicImage,
    pub exif: HashMap<String, String>,
    pub load_time: Duration,
    pub path: PathBuf,
}

pub fn load_image(path: &Path) -> Result<LoadedImage> {
    let start_time = Instant::now();
    let extension = path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
        .unwrap_or_default();

    let (image, exif) = match extension.as_str() {
        "nef" | "cr2" | "dng" | "arw" => load_raw(path)?,
        _ => load_standard(path)?,
    };

    // Try to read orientation for RAW files too if not already handled (load_standard handles it internally now, but let's refactor)
    // Actually, let's refactor so both return image and we apply orientation after.
    // But load_standard reads from buffer, load_raw reads from path.
    
    // Let's just make sure load_raw applies orientation.
    
    let load_time = start_time.elapsed();

    Ok(LoadedImage {
        image,
        exif,
        load_time,
        path: path.to_path_buf(),
    })
}



fn load_standard(path: &Path) -> Result<(DynamicImage, HashMap<String, String>)> {
    let mut file = std::fs::File::open(path)?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut buf)?;
    
    let mut img = image::load_from_memory(&buf).map_err(|e| anyhow!(e))?;
    
    let mut exif_map = HashMap::new();
    let reader = Reader::new();
    
    // Extract EXIF data
    if let Ok(exif) = reader.read_from_container(&mut Cursor::new(&buf)) {
        for field in exif.fields() {
            let key = field.tag.to_string();
            let value = field.display_value().with_unit(&exif).to_string();
            exif_map.insert(key, value);
        }
        
        if let Some(field) = exif.get_field(Tag::Orientation, In::PRIMARY) {
            if let Value::Short(ref v) = field.value {
                if let Some(&orientation) = v.first() {
                    println!("Found orientation: {}", orientation);
                    img = apply_orientation(img, orientation as u32);
                }
            }
        }
    }

    Ok((img, exif_map))
}

fn load_raw(path: &Path) -> Result<(DynamicImage, HashMap<String, String>)> {
    let loader = rawloader::RawLoader::new();
    let raw = loader.decode_file(path).map_err(|e| anyhow!(e))?;

    let (width, height) = (raw.width, raw.height);
    
    let mut exif_map = HashMap::new();
    exif_map.insert("Make".to_string(), raw.make.clone());
    exif_map.insert("Model".to_string(), raw.model.clone());
    
    let data_u16: Vec<u16> = if let rawloader::RawImageData::Integer(data) = raw.data {
        data
    } else {
        return Err(anyhow!("Unsupported raw data format"));
    };

    let pattern = raw.cfa.name.as_str();
    
    let rgb_u8 = demosaic_bilinear(
        &data_u16, 
        width, 
        height, 
        pattern, 
        &raw.whitelevels, 
        &raw.blacklevels, 
        &raw.wb_coeffs
    );

    let buffer: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_raw(width as u32, height as u32, rgb_u8)
        .ok_or_else(|| anyhow!("Failed to create image buffer"))?;
        
    let mut img = DynamicImage::ImageRgb8(buffer);
    
    // Try to read EXIF from the file to get orientation
    // We read the file header/content to find EXIF
    if let Ok(file) = std::fs::File::open(path) {
        // Read enough bytes? Or whole file? RAW files are large.
        // kamadak-exif might need the whole file if EXIF is at the end?
        // Usually EXIF is at the beginning.
        // Let's try reading the first 1MB.
        // But read_from_container takes a Reader (Seek + Read).
        // We can just pass the file!
        
        let reader = Reader::new();
        if let Ok(exif) = reader.read_from_container(&mut std::io::BufReader::new(file)) {
             if let Some(field) = exif.get_field(Tag::Orientation, In::PRIMARY) {
                if let Value::Short(ref v) = field.value {
                    if let Some(&orientation) = v.first() {
                        println!("Found RAW orientation: {}", orientation);
                        img = apply_orientation(img, orientation as u32);
                    }
                }
            }
        }
    }

    Ok((img, exif_map))
}

fn apply_orientation(img: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;

    #[test]
    fn test_apply_orientation() {
        let img = DynamicImage::new_rgb8(10, 20);
        
        // Case 1: Normal (no change)
        let res = apply_orientation(img.clone(), 1);
        assert_eq!(res.dimensions(), (10, 20));

        // Case 6: Rotate 90 CW
        let res = apply_orientation(img.clone(), 6);
        assert_eq!(res.dimensions(), (20, 10));
        
        // Case 8: Rotate 270 CW (90 CCW)
        let res = apply_orientation(img.clone(), 8);
        assert_eq!(res.dimensions(), (20, 10));
        
        // Case 3: Rotate 180
        let res = apply_orientation(img.clone(), 3);
        assert_eq!(res.dimensions(), (10, 20));
    }

    #[test]
    fn test_color_rendering() {
        // Simulate a 2x2 RGGB pattern with pure Blue
        // R G
        // G B
        // Let's make it 4x4 to avoid boundary issues with the demosaic loop (it skips 1 pixel border)
        let width = 4;
        let height = 4;
        let mut data = vec![0u16; width * height];
        
        // Fill with "Blue" signal
        // In RGGB:
        // Row 0: R G R G
        // Row 1: G B G B
        // Row 2: R G R G
        // Row 3: G B G B
        
        // We want pure blue, so only B pixels have value.
        // B pixels are at odd row, odd col.
        for y in 0..height {
            for x in 0..width {
                if y % 2 == 1 && x % 2 == 1 {
                    data[y * width + x] = 1000; // Blue signal
                } else {
                    data[y * width + x] = 0; // No signal
                }
            }
        }
        
        let whitelevels = vec![1000, 1000, 1000, 1000];
        let blacklevels = vec![0, 0, 0, 0];
        let wb_coeffs = vec![1.0, 1.0, 1.0, 1.0]; // Neutral WB
        
        let rgb = demosaic_bilinear(
            &data,
            width,
            height,
            "RGGB",
            &whitelevels,
            &blacklevels,
            &wb_coeffs
        );
        
        // Check center pixel (1, 1) - should be Blue
        // Index: (1 * 4 + 1) * 3 = 15
        let idx = (1 * 4 + 1) * 3;
        let r = rgb[idx];
        let g = rgb[idx+1];
        let b = rgb[idx+2];
        
        println!("RGB at (1,1): {}, {}, {}", r, g, b);
        
        // With current logic:
        // B at (1,1) is 1000. Normalized: 1.0. Gamma: 1.0. Output: 255.
        // G at (1,1) is avg of neighbors (0,1), (1,0), (1,2), (2,1). All 0. Output: 0.
        // R at (1,1) is avg of (0,0), (0,2), (2,0), (2,2). All 0. Output: 0.
        // So it should be pure blue (0, 0, 255).
        
        // However, real cameras have color crosstalk and need a matrix.
        // If we had a matrix, this pure blue camera signal might map to something else in sRGB.
        // But for this test, we just verify the pipeline works as expected.
        
        assert_eq!(b, 255);
        assert_eq!(r, 0);
        assert_eq!(g, 0);
    }
}

fn demosaic_bilinear(
    input: &[u16], 
    width: usize, 
    height: usize, 
    pattern: &str, 
    whitelevels: &[u16], 
    blacklevels: &[u16], 
    wb_coeffs: &[f32]
) -> Vec<u8> {
    let mut output = vec![0u8; width * height * 3];
    
    let r_gain = wb_coeffs[0];
    let g_gain = wb_coeffs[1];
    let b_gain = wb_coeffs[2];
    
    let bl_r = blacklevels[0] as f32;
    let bl_g = blacklevels[1] as f32;
    let bl_b = blacklevels[2] as f32;
    
    let wl_r = whitelevels[0] as f32;
    let wl_g = whitelevels[1] as f32;
    let wl_b = whitelevels[2] as f32;
    
    let range_r = wl_r - bl_r;
    let range_g = wl_g - bl_g;
    let range_b = wl_b - bl_b;

    let get = |x: usize, y: usize| -> f32 {
        if x >= width || y >= height {
            0.0
        } else {
            input[y * width + x] as f32
        }
    };

    for y in 1..height-1 {
        for x in 1..width-1 {
            let idx = (y * width + x) * 3;
            let row = y % 2;
            let col = x % 2;
            
            let (r, g, b) = match pattern {
                "RGGB" => match (row, col) {
                    (0, 0) => {
                        let r = get(x, y);
                        let g = (get(x-1, y) + get(x+1, y) + get(x, y-1) + get(x, y+1)) / 4.0;
                        let b = (get(x-1, y-1) + get(x+1, y-1) + get(x-1, y+1) + get(x+1, y+1)) / 4.0;
                        (r, g, b)
                    },
                    (0, 1) => {
                        let r = (get(x-1, y) + get(x+1, y)) / 2.0;
                        let g = get(x, y);
                        let b = (get(x, y-1) + get(x, y+1)) / 2.0;
                        (r, g, b)
                    },
                    (1, 0) => {
                        let r = (get(x, y-1) + get(x, y+1)) / 2.0;
                        let g = get(x, y);
                        let b = (get(x-1, y) + get(x+1, y)) / 2.0;
                        (r, g, b)
                    },
                    (1, 1) => {
                        let r = (get(x-1, y-1) + get(x+1, y-1) + get(x-1, y+1) + get(x+1, y+1)) / 4.0;
                        let g = (get(x-1, y) + get(x+1, y) + get(x, y-1) + get(x, y+1)) / 4.0;
                        let b = get(x, y);
                        (r, g, b)
                    },
                    _ => (0.0, 0.0, 0.0),
                },
                "BGGR" => match (row, col) {
                    (0, 0) => {
                        let b = get(x, y);
                        let g = (get(x-1, y) + get(x+1, y) + get(x, y-1) + get(x, y+1)) / 4.0;
                        let r = (get(x-1, y-1) + get(x+1, y-1) + get(x-1, y+1) + get(x+1, y+1)) / 4.0;
                        (r, g, b)
                    },
                    (0, 1) => {
                        let b = (get(x-1, y) + get(x+1, y)) / 2.0;
                        let g = get(x, y);
                        let r = (get(x, y-1) + get(x, y+1)) / 2.0;
                        (r, g, b)
                    },
                    (1, 0) => {
                        let b = (get(x, y-1) + get(x, y+1)) / 2.0;
                        let g = get(x, y);
                        let r = (get(x-1, y) + get(x+1, y)) / 2.0;
                        (r, g, b)
                    },
                    (1, 1) => {
                        let b = (get(x-1, y-1) + get(x+1, y-1) + get(x-1, y+1) + get(x+1, y+1)) / 4.0;
                        let g = (get(x-1, y) + get(x+1, y) + get(x, y-1) + get(x, y+1)) / 4.0;
                        let r = get(x, y);
                        (r, g, b)
                    },
                    _ => (0.0, 0.0, 0.0),
                },
                _ => {
                     let val = get(x, y);
                     (val, val, val)
                }
            };

            let r_norm = ((r - bl_r).max(0.0) / range_r) * r_gain;
            let g_norm = ((g - bl_g).max(0.0) / range_g) * g_gain;
            let b_norm = ((b - bl_b).max(0.0) / range_b) * b_gain;

            // Apply a simple color matrix for better color rendering
            // This is a simplified sRGB-like matrix to improve color accuracy
            let r_corrected = (1.6 * r_norm - 0.3 * g_norm - 0.3 * b_norm).max(0.0).min(1.0);
            let g_corrected = (-0.2 * r_norm + 1.4 * g_norm - 0.2 * b_norm).max(0.0).min(1.0);
            let b_corrected = (-0.1 * r_norm - 0.3 * g_norm + 1.4 * b_norm).max(0.0).min(1.0);

            // Apply gamma correction
            let r_gamma = r_corrected.powf(1.0 / 2.2);
            let g_gamma = g_corrected.powf(1.0 / 2.2);
            let b_gamma = b_corrected.powf(1.0 / 2.2);

            output[idx] = (r_gamma * 255.0).min(255.0) as u8;
            output[idx + 1] = (g_gamma * 255.0).min(255.0) as u8;
            output[idx + 2] = (b_gamma * 255.0).min(255.0) as u8;
        }
    }
    output
}
