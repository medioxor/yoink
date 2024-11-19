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
#[cfg(target_os = "windows")]
mod reader_windows;
#[cfg(target_os = "windows")]
use reader_windows::read_file;
use std::fs;

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
        println!("Searching path: {}", path);
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
        let mut files: Vec<String> = Vec::new();
        let ignored = vec!["/proc", "/dev", "/sys"];
        if rule.path.starts_with("**") {
            for entry in fs::read_dir("/")? {
                let path = entry?.path();
                if path.is_dir() && !ignored.contains(&path.to_str().ok_or("invalid path")?) {
                    let search_path = format!("{}/{}", path.display(), rule.path);
                    files.append(&mut Collecter::search_filesystem(&search_path)?);
                }
            }
            return Ok(files)
        }
        else {
            Ok(Collecter::search_filesystem(rule.path.as_str())?)
        }
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
        let zip_file = File::create(output_file)?;
        let mut zip = ZipWriter::new(zip_file);

        for file in &self.files {
            #[cfg(target_os = "linux")]
            let last_modify_time = File::open(&file)?.metadata()?.modified()?;
            #[cfg(target_os = "linux")]
            let last_modify_time = DateTime::<Local>::from(last_modify_time).naive_utc();
            #[cfg(target_os = "linux")]
            let file_contents = std::fs::read(file)?;
            #[cfg(target_os = "windows")]
            let (file_contents, last_modify_time) = read_file(std::path::Path::new(file))?;

            if self.encryption_key.is_some() {
                // unwrap safe because we already checked if some
                let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::BZIP2).last_modified_time(last_modify_time.try_into()?).with_aes_encryption(Aes256, self.encryption_key.as_deref().unwrap());
                #[cfg(target_os = "windows")]
                zip.start_file_from_path(file.replace(":", ""), options)?;
                #[cfg(target_os = "linux")]
                zip.start_file_from_path(file, options)?;
            }
            else {
                let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::BZIP2).last_modified_time(last_modify_time.try_into()?);
                #[cfg(target_os = "windows")]
                zip.start_file_from_path(file.replace(":", ""), options)?;
                #[cfg(target_os = "linux")]
                zip.start_file_from_path(file, options)?;
            }

            zip.write_all(&file_contents)?;
        }
        
        zip.finish()?;
        Ok(())
    }

}