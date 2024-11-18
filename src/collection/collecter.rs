mod rules;
use chrono::{DateTime, Local};
use rules::CollectionRule;
use std::io::Write;
use std::{error::Error, fs::File};
use zip::ZipWriter;
use zip::AesMode::Aes256;
use glob::glob;

#[cfg(target_os = "windows")]
use windows::Win32::Storage::FileSystem::GetLogicalDriveStringsA;

pub struct Collecter {
    rules: Vec<CollectionRule>,
    encryption_key: Option<String>,
    files: Vec<String>
}

impl Collecter {
    pub fn new(platform: String, encryption_key: Option<String>) -> Result<Self, Box<dyn Error>> {
        Ok(Collecter {
            rules: CollectionRule::get_rules_by_platform(platform.as_str())?,
            encryption_key: encryption_key,
            files: Vec::new()
        })
    }

    fn search_filesystem(path: &str) -> Result<Vec<String>, Box<dyn Error>> {
        let mut files = Vec::new();
        glob(path)?.for_each(|entry| {
            match entry {
                Ok(path) => {
                    if path.is_file() {
                        if let Ok(file_path) = path.into_os_string().into_string() {
                            println!("Found file: {}", file_path);
                            files.push(file_path);
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
        Ok(files)
    }

    #[cfg(target_os = "linux")]
    pub fn collect_by_rule(rule: &CollectionRule) -> Result<Vec<String>, Box<dyn Error>> {
        files.append(&mut Collecter::search_filesystem(rule.path.as_str())?);
        Ok(files)
    }

    #[cfg(target_os = "windows")]
    pub fn collect_by_rule(rule: &CollectionRule) -> Result<Vec<String>, Box<dyn Error>> {
        let drives = Collecter::get_windows_drives()?;
        let mut files = Vec::new();
        for drive in drives {
            println!("Searching drive: {}", drive);
            let path = format!("{drive}{0}", rule.path);
            files.append(&mut Collecter::search_filesystem(path.as_str())?);
        }
        Ok(files)
    }

    #[cfg(target_os = "windows")]
    fn get_windows_drives() -> Result<Vec<String>, Box<dyn Error>> {
        let mut drives = Vec::new();
        let mut buffer = [0u8; 255];
        let result = unsafe { GetLogicalDriveStringsA(Some(&mut buffer)) };
        if result == 0 {
            return Err("Failed to get logical drives".into());
        }
        let drives_string = String::from_utf8(buffer.to_vec())?;
        for drive in drives_string.split("\0") {
            if drive.len() > 0 {
                drives.push(drive.to_string());
            }
        }
        Ok(drives)
    }

    pub fn collect_all(&mut self) -> Result<(), Box<dyn Error>> {
        for rule in &self.rules {
            self.files.append(&mut Collecter::collect_by_rule(rule)?);
        }
        Ok(())
    }

    pub fn compress_collection(&self, output_file: &str) -> Result<(), Box<dyn Error>> {
        let file = File::create(output_file)?;
        let mut zip = ZipWriter::new(file);
        
        for file in &self.files {
            let last_modify_time = File::open(&file)?.metadata()?.modified()?;
            let last_modify_time = DateTime::<Local>::from(last_modify_time).naive_utc();
            if self.encryption_key.is_some() {
                // unwrap safe because we already checked if some
                let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::BZIP2).last_modified_time(last_modify_time.try_into()?).with_aes_encryption(Aes256, self.encryption_key.as_deref().unwrap());
                zip.start_file_from_path(file, options)?;
            }
            else {
                let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::BZIP2).last_modified_time(last_modify_time.try_into()?);
                zip.start_file_from_path(file, options)?;
            }
            
            let file_contents = std::fs::read(file)?;
            zip.write_all(&file_contents)?;
        }
        
        zip.finish()?;
        Ok(())
    }

}