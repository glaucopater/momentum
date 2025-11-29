use std::path::{Path, PathBuf};

pub struct Navigator {
    pub current_path: Option<PathBuf>,
    pub image_list: Vec<PathBuf>,
}

impl Navigator {
    pub fn new() -> Self {
        Self {
            current_path: None,
            image_list: Vec::new(),
        }
    }

    pub fn update_file_list(&mut self, path: &Path) {
        self.current_path = Some(path.to_path_buf());
        
        let parent = match path.parent() {
            Some(p) => p,
            None => return,
        };
        
        let needs_update = if let Some(first) = self.image_list.first() {
            first.parent() != Some(parent)
        } else {
            true
        };
        
        if needs_update {
            let mut list = Vec::new();
            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) {
                            match ext.as_str() {
                                "jpg" | "jpeg" | "png" | "nef" | "cr2" | "dng" | "arw" => {
                                    list.push(path);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            list.sort();
            self.image_list = list;
        }
    }
    
    pub fn get_next_image(&self) -> Option<PathBuf> {
        if let Some(current) = &self.current_path {
            if let Some(pos) = self.image_list.iter().position(|p| p == current) {
                if pos + 1 < self.image_list.len() {
                    return Some(self.image_list[pos + 1].clone());
                }
            }
        }
        None
    }
    
    pub fn get_prev_image(&self) -> Option<PathBuf> {
        if let Some(current) = &self.current_path {
            if let Some(pos) = self.image_list.iter().position(|p| p == current) {
                if pos > 0 {
                    return Some(self.image_list[pos - 1].clone());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_navigation() {
        let mut nav = Navigator::new();
        let p1 = PathBuf::from("a.jpg");
        let p2 = PathBuf::from("b.jpg");
        let p3 = PathBuf::from("c.jpg");
        
        nav.image_list = vec![p1.clone(), p2.clone(), p3.clone()];
        
        // Test Next
        nav.current_path = Some(p1.clone());
        assert_eq!(nav.get_next_image(), Some(p2.clone()));
        
        nav.current_path = Some(p2.clone());
        assert_eq!(nav.get_next_image(), Some(p3.clone()));
        
        nav.current_path = Some(p3.clone());
        assert_eq!(nav.get_next_image(), None);
        
        // Test Prev
        nav.current_path = Some(p3.clone());
        assert_eq!(nav.get_prev_image(), Some(p2.clone()));
        
        nav.current_path = Some(p2.clone());
        assert_eq!(nav.get_prev_image(), Some(p1.clone()));
        
        nav.current_path = Some(p1.clone());
        assert_eq!(nav.get_prev_image(), None);
    }
}
