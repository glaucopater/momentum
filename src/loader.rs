use anyhow::{anyhow, Result};
use image::{DynamicImage, ImageBuffer, Rgb};
use std::path::Path;
use std::time::{Duration, Instant};
use std::collections::HashMap;

#[derive(Debug)]
pub struct LoadedImage {
    pub image: DynamicImage,
    pub exif: HashMap<String, String>,
    pub load_time: Duration,
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

    let load_time = start_time.elapsed();

    Ok(LoadedImage {
        image,
        exif,
        load_time,
    })
}

fn load_standard(path: &Path) -> Result<(DynamicImage, HashMap<String, String>)> {
    let img = image::open(path).map_err(|e| anyhow!(e))?;
    
    let exif_map = HashMap::new();
    
    // TODO: Add EXIF extraction for JPEG files using kamadak-exif

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

    Ok((DynamicImage::ImageRgb8(buffer), exif_map))
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

            let r_gamma = r_norm.powf(1.0 / 2.2);
            let g_gamma = g_norm.powf(1.0 / 2.2);
            let b_gamma = b_norm.powf(1.0 / 2.2);

            output[idx] = (r_gamma * 255.0).min(255.0) as u8;
            output[idx + 1] = (g_gamma * 255.0).min(255.0) as u8;
            output[idx + 2] = (b_gamma * 255.0).min(255.0) as u8;
        }
    }
    output
}
