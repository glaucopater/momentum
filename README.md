# Momemtum Image Viewer

Momemtum is a high-performance image viewer written in Rust, designed for speed and simplicity. It supports standard image formats (JPEG, PNG) as well as RAW formats (NEF, CR2, DNG, ARW).

## Features

-   **Fast Loading:** Optimized for quick image loading and rendering.
-   **RAW Support:** Native support for various RAW image formats.
-   **Auto-Rotation:** Automatically rotates images based on EXIF orientation tags.
-   **Folder Navigation:** Seamlessly navigate through images in a folder using arrow keys.
-   **Minimalist UI:** Clean interface with essential information (Zoom, Load Time, Memory Usage, EXIF Model) displayed in the title bar.
-   **GPU Acceleration:** Uses `wgpu` for hardware-accelerated rendering.

## Installation

Ensure you have Rust and Cargo installed.

```bash
git clone https://github.com/glaucopater/momentum.git
cd momentum
cargo build --release
```

## Usage

Run the application:

```bash
cargo run --release
```

Or drag and drop an image file onto the executable or the running window.

### Controls

-   **Drag & Drop:** Open an image.
-   **Left Arrow:** View previous image in the folder.
-   **Right Arrow:** View next image in the folder.
-   **Mouse Wheel:** Zoom in/out.
-   **Left Click + Drag:** Pan the image.
-   **Escape:** Exit the application.

## License

[MIT License](LICENSE)
