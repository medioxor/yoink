mod rules;
use rules::CollectionRule;
use std::{error::Error, fs::File};
use zip::ZipWriter;
use zip::AesMode::Aes256;
use glob::glob;

pub struct Collecter {
    rules: Vec<CollectionRule>,
    encryption_key: Option<String>,
    files: Vec<String>
}

impl Collecter {
    pub fn new_encrypted(platform: String, encryption_key: String) -> Result<Self, Box<dyn Error>> {
        Ok(Collecter {
            rules: CollectionRule::get_rules_by_platform(platform.as_str())?,
            encryption_key: Some(encryption_key),
            files: Vec::new()
        })
    }

    pub fn new(platform: String) -> Result<Self, Box<dyn Error>> {
        Ok(Collecter {
            rules: CollectionRule::get_rules_by_platform(platform.as_str())?,
            encryption_key: None,
            files: Vec::new()
        })
    }

    #[cfg(target_os = "linux")]
    pub fn collect(&mut self) -> Result<(), Box<dyn Error>> {
        for rule in &self.rules {
            glob(&rule.path)?.for_each(|entry| {
                match entry {
                    Ok(path) => {
                        if path.is_file() {
                            if let Ok(file_path) = path.into_os_string().into_string() {
                                self.files.push(file_path);
                            } else {
                                println!("Failed to convert path to string");
                            }
                        }
                    }
                    Err(e) => {
                        println!("Error: {}", e);
                    }
                }
            });
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    pub fn collect(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

}