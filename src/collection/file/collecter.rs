use super::rules::CollectionRule;
use glob::glob;
use std::{env, error::Error};

use super::rules::FileRule;

#[cfg(target_os = "windows")]
use super::reader::ntfs_reader::parse_stream;
#[cfg(target_os = "windows")]
use windows::Win32::Storage::FileSystem::GetLogicalDriveStringsA;

pub struct FileCollecter {
    rules: Vec<FileRule>,
    pub files: Vec<String>,
}

impl FileCollecter {
    pub fn new(platform: String) -> Result<Self, Box<dyn Error>> {
        Ok(FileCollecter {
            rules: CollectionRule::get_rules_by_platform_and_type(platform.as_str(), "file")?
                .into_iter()
                .filter_map(|rule| {
                    if let CollectionRule::FileRule(rule) = rule {
                        Some(rule)
                    } else {
                        None
                    }
                })
                .collect(),
            files: Vec::new(),
        })
    }

    pub fn add_rule(&mut self, new_rule: CollectionRule) -> Result<(), Box<dyn Error>> {
        if let CollectionRule::FileRule(rule) = new_rule {
            if rule.platform != env::consts::OS {
                return Err("Rule platform does not match current platform".into());
            }
            if self
                .rules
                .iter()
                .any(|existing_rule| existing_rule.name == rule.name)
            {
                return Err("Rule with this name already exists".into());
            }
            self.rules.push(rule);
        } else {
            return Err("Only file rules can be added".into());
        }
        Ok(())
    }

    fn search_filesystem(path: &str) -> Result<Vec<String>, Box<dyn Error>> {
        let mut files = Vec::new();
        println!("Searching path: {}", path);
        glob(path)?.for_each(|entry| match entry {
            Ok(path) => {
                if path.is_file() {
                    if let Ok(file_path) = path.into_os_string().into_string() {
                        println!("Found artefact: {}", file_path);
                        files.push(file_path);
                    } else {
                        println!("Failed to convert path to string");
                    }
                }
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        });
        Ok(files)
    }

    #[cfg(target_os = "linux")]
    pub fn collect_by_rule(rule: &FileRule) -> Result<Vec<String>, Box<dyn Error>> {
        let mut files: Vec<String> = Vec::new();
        let ignored = ["/proc", "/dev", "/sys"];
        if rule.path.starts_with("**") {
            for entry in std::fs::read_dir("/")? {
                let path = entry?.path();
                if path.is_dir() && !ignored.contains(&path.to_str().ok_or("invalid path")?) {
                    let search_path = format!("{}/{}", path.display(), rule.path);
                    files.append(&mut FileCollecter::search_filesystem(&search_path)?);
                }
            }
            Ok(files)
        } else {
            Ok(FileCollecter::search_filesystem(rule.path.as_str())?)
        }
    }

    pub fn collect_by_rulename(&mut self, rule_name: &str) -> Result<usize, Box<dyn Error>> {
        let rule = self
            .rules
            .iter()
            .find(|rule| rule.name == rule_name)
            .ok_or_else(|| format!("Rule with name '{}' not found", rule_name))?;
        let mut collected_files = FileCollecter::collect_by_rule(rule)?;
        let collected_files_len = collected_files.len();
        self.files.append(&mut collected_files);
        Ok(collected_files_len)
    }

    #[cfg(target_os = "windows")]
    pub fn collect_by_rule(rule: &FileRule) -> Result<Vec<String>, Box<dyn Error>> {
        let drives = FileCollecter::get_windows_drives()?;
        let mut files = Vec::new();
        for drive in drives {
            println!("Searching drive: {}", drive);
            let path = format!("{drive}{0}", rule.path);
            if path.chars().filter(|&c| c == ':').count() >= 2 {
                let (path, stream) = parse_stream(path.as_str());
                if path.contains("*") {
                    for file in &mut FileCollecter::search_filesystem(path.as_str())? {
                        files.push(format!("{}:{}", file, stream));
                    }
                } else {
                    files.push(format!("{}:{}", path, stream));
                }
            } else if path.contains("*") {
                files.append(&mut FileCollecter::search_filesystem(path.as_str())?);
            } else {
                files.push(path);
            }
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
            if !drive.is_empty() {
                drives.push(drive.to_string());
            }
        }
        Ok(drives)
    }

    pub fn collect_all(&mut self) -> Result<(), Box<dyn Error>> {
        for rule in &self.rules {
            match FileCollecter::collect_by_rule(rule) {
                Ok(mut files) => {
                    println!(
                        "Collected {0} artefacts for rule: {1}",
                        self.files.len(),
                        rule.name
                    );
                    self.files.append(&mut files);
                }
                Err(e) => println!("Failed to collect artefacts for rule: {}\n{}", rule.name, e),
            }
        }
        Ok(())
    }
}
