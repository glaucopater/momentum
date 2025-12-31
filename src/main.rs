#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod state;
mod texture;
mod loader;
mod navigator;
use state::State;
use winit::{
    event::*,
    event_loop::EventLoopBuilder,
    window::WindowBuilder,
};

use crate::loader::LoadedImage;

#[derive(Debug)]
enum AppEvent {
    ImageLoaded(LoadedImage),
}

fn main() {
    env_logger::init();
    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build().unwrap();
    let window = WindowBuilder::new().build(&event_loop).unwrap();
    window.set_title("Momemtum Image Viewer");
    
    // Set window icon from assets/icon.ico
    {
        use std::fs::File;
        use std::io::BufReader;
        
        let icon_path = std::path::Path::new("assets").join("icon.ico");
        if let Ok(file) = File::open(&icon_path) {
            let reader = BufReader::new(file);
            if let Ok(icon_dir) = ico::IconDir::read(reader) {
                // Get the largest icon entry
                if let Some(entry) = icon_dir.entries().iter().max_by_key(|e| e.width()) {
                    if let Ok(image) = entry.decode() {
                        let width = image.width();
                        let height = image.height();
                        let rgba_data = image.rgba_data().to_vec();
                        
                        if let Ok(icon) = winit::window::Icon::from_rgba(rgba_data, width, height) {
                            window.set_window_icon(Some(icon));
                        }
                    }
                }
            }
        }
    }

    let event_loop_proxy = event_loop.create_proxy();

    let mut state = pollster::block_on(State::new(&window));

    event_loop.run(move |event, elwt| {
        match event {
            Event::UserEvent(AppEvent::ImageLoaded(loaded_image)) => {
                state.set_image(loaded_image);
            }
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == state.window.id() => {
                if !state.input(event) {
                    match event {
                        WindowEvent::CloseRequested => elwt.exit(),
                        WindowEvent::KeyboardInput {
                            event:
                                KeyEvent {
                                    state: ElementState::Pressed,
                                    physical_key: winit::keyboard::PhysicalKey::Code(keycode),
                                    ..
                                },
                            ..
                        } => {
                            match keycode {
                                winit::keyboard::KeyCode::Escape => elwt.exit(),
                                winit::keyboard::KeyCode::ArrowLeft => {
                                    if let Some(path) = state.get_prev_image() {
                                        let proxy = event_loop_proxy.clone();
                                        std::thread::spawn(move || {
                                            match crate::loader::load_image(&path) {
                                                Ok(img) => {
                                                    let _ = proxy.send_event(AppEvent::ImageLoaded(img));
                                                }
                                                Err(e) => {
                                                    eprintln!("Failed to load image: {:?}", e);
                                                }
                                            }
                                        });
                                    }
                                }
                                winit::keyboard::KeyCode::ArrowRight => {
                                    if let Some(path) = state.get_next_image() {
                                        let proxy = event_loop_proxy.clone();
                                        std::thread::spawn(move || {
                                            match crate::loader::load_image(&path) {
                                                Ok(img) => {
                                                    let _ = proxy.send_event(AppEvent::ImageLoaded(img));
                                                }
                                                Err(e) => {
                                                    eprintln!("Failed to load image: {:?}", e);
                                                }
                                            }
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                        WindowEvent::Resized(physical_size) => {
                            state.resize(*physical_size);
                        }
                        WindowEvent::DroppedFile(path) => {
                            let proxy = event_loop_proxy.clone();
                            let path = path.to_owned();
                            std::thread::spawn(move || {
                                match crate::loader::load_image(&path) {
                                    Ok(img) => {
                                        let _ = proxy.send_event(AppEvent::ImageLoaded(img));
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to load image: {:?}", e);
                                    }
                                }
                            });
                        }
                        WindowEvent::RedrawRequested => {
                            state.update();
                            match state.render() {
                                Ok(_) => {}
                                Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                                Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                                Err(e) => eprintln!("{:?}", e),
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::AboutToWait => {
                state.window.request_redraw();
            }
            _ => {}
        }
    }).unwrap();
}
