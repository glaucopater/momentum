fn main() {
    #[cfg(windows)]
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let ico_path = std::path::Path::new(&manifest_dir).join("assets").join("icon.ico");
        
        let mut res = winres::WindowsResource::new();
        
        // Use existing icon from assets folder
        if ico_path.exists() {
            println!("cargo:warning=Using icon from assets/icon.ico");
            res.set_icon(ico_path.to_str().unwrap());
        } else {
            println!("cargo:warning=assets/icon.ico not found, skipping icon embedding");
        }
        
        res.compile().unwrap();
    }
}

