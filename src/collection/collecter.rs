use super::rules::CollectionRule;
use chrono::NaiveDateTime;
use glob::glob;
use std::{env, error::Error, fs::File};
use zip::{
    write::{FileOptions, SimpleFileOptions},
    AesMode::Aes256,
    CompressionMethod, ZipWriter,
};

#[cfg(target_os = "windows")]
use windows::Win32::Storage::FileSystem::GetLogicalDriveStringsA;
#[cfg(target_os = "windows")]
use super::windows_reader::{copy_file, get_lastmodified, parse_stream};

pub struct Collecter {
    rules: Vec<CollectionRule>,
    encryption_key: Option<String>,
    files: Vec<String>,
}

impl Collecter {
    pub fn new(platform: String, encryption_key: Option<String>) -> Result<Self, Box<dyn Error>> {
        Ok(Collecter {
            rules: CollectionRule::get_rules_by_platform(platform.as_str())?,
            encryption_key,
            files: Vec::new(),
        })
    }

    pub fn add_rule_from_file(&mut self, file_path: &str) -> Result<(), Box<dyn Error>> {
        let new_rule = CollectionRule::from_yaml_file(file_path)?;
        if self.rules.iter().any(|rule| rule.name == new_rule.name) {
            return Ok(());
        }
        if new_rule.platform != env::consts::OS {
            return Ok(());
        }
        self.rules.push(new_rule);
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
    pub fn collect_by_rule(rule: &CollectionRule) -> Result<Vec<String>, Box<dyn Error>> {
        let mut files: Vec<String> = Vec::new();
        let ignored = ["/proc", "/dev", "/sys"];
        if rule.path.starts_with("**") {
            for entry in std::fs::read_dir("/")? {
                let path = entry?.path();
                if path.is_dir() && !ignored.contains(&path.to_str().ok_or("invalid path")?) {
                    let search_path = format!("{}/{}", path.display(), rule.path);
                    files.append(&mut Collecter::search_filesystem(&search_path)?);
                }
            }
            Ok(files)
        } else {
            Ok(Collecter::search_filesystem(rule.path.as_str())?)
        }
    }

    pub fn collect_by_rulename(&mut self, rule_name: &str) -> Result<usize, Box<dyn Error>> {
        let rule = self
            .rules
            .iter()
            .find(|rule| rule.name == rule_name)
            .ok_or_else(|| format!("Rule with name '{}' not found", rule_name))?;
        let mut collected_files = Collecter::collect_by_rule(rule)?;
        let collected_files_len = collected_files.len();
        self.files.append(&mut collected_files);
        Ok(collected_files_len)
    }

    #[cfg(target_os = "windows")]
    pub fn collect_by_rule(rule: &CollectionRule) -> Result<Vec<String>, Box<dyn Error>> {
        let drives = Collecter::get_windows_drives()?;
        let mut files = Vec::new();
        for drive in drives {
            println!("Searching drive: {}", drive);
            let path = format!("{drive}{0}", rule.path);
            if path.chars().filter(|&c| c == ':').count() >= 2 {
                let (path, stream) = parse_stream(path.as_str());
                println!("path: {}, stream: {}", path, stream);
                if path.contains("*") {
                    for file in &mut Collecter::search_filesystem(path.as_str())? {
                        files.push(format!("{}:{}", file, stream));
                    }
                } else {
                    files.push(format!("{}:{}", path, stream));
                }
            } else if path.contains("*") {
                files.append(&mut Collecter::search_filesystem(path.as_str())?);
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
            match Collecter::collect_by_rule(rule) {
                Ok(mut files) => {
                    println!("Collected artefacts for rule: {}", rule.name);
                    self.files.append(&mut files);
                }
                Err(e) => println!("Failed to collect artefacts for rule: {}\n{}", rule.name, e),
            }
        }
        Ok(())
    }

    fn get_zip_options(
        &mut self,
        last_modified: NaiveDateTime,
    ) -> Result<FileOptions<'_, ()>, Box<dyn Error>> {
        if self.encryption_key.is_some() {
            Ok(SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::BZIP2)
                .last_modified_time(last_modified.try_into()?)
                .large_file(true)
                .with_aes_encryption(Aes256, self.encryption_key.as_deref().unwrap()))
        } else {
            Ok(SimpleFileOptions::default()
                .compression_method(CompressionMethod::BZIP2)
                .large_file(true)
                .last_modified_time(last_modified.try_into()?))
        }
    }

    fn compress_file(
        &mut self,
        zip: &mut ZipWriter<File>,
        file_path: String,
    ) -> Result<(), Box<dyn Error>> {
        println!("Compressing file: {}", file_path);
        let (path, stream_name) = parse_stream(file_path.as_str());

        let last_modified = get_lastmodified(path.clone())?;
        let options = self.get_zip_options(last_modified)?;

        if stream_name.is_empty() {
            zip.start_file_from_path(path.replace(":", ""), options)?;
        } else {
            zip.start_file_from_path(
                format!("{0}_{1}", path.replace(":", ""), stream_name),
                options,
            )?;
        }

        copy_file(file_path, zip)?;

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn compress_file(zip: &mut ZipWriter<File>, file_path: String) -> Result<(), Box<dyn Error>> {
        println!("Compressing file: {}", file_path);

        let file = File::options()
            .read(true)
            .write(false)
            .open(file_path.clone())?;
        let last_modified = file.metadata()?.modified()?;
        let mut reader = BufReader::new(file);
        let last_modified = DateTime::<Local>::from(last_modified).naive_utc();
        let options = Collecter::get_zip_options(last_modified)?;

        zip.start_file_from_path(file_path, options)?;

        loop {
            let length = {
                let buffer = reader.fill_buf()?;
                zip.write_all(buffer)?;
                buffer.len()
            };
            if length == 0 {
                break;
            }
            reader.consume(length);
        }

        Ok(())
    }

    pub fn compress_collection(&mut self, output_file: &str) -> Result<(), Box<dyn Error>> {
        let zip_file = File::create(output_file)?;
        let mut zip: ZipWriter<File> = ZipWriter::new(zip_file);

        if self.files.is_empty() {
            return Err("No artefacts to compress".into());
        }

        // remove duplicate files
        let mut unique_files = std::collections::HashSet::new();
        self.files.retain(|file| unique_files.insert(file.clone()));
        let files_to_compress = self.files.clone();

        for file_path in files_to_compress {
            match self.compress_file(&mut zip, file_path.clone()) {
                Ok(_) => {
                    println!("Compressed file: {}", file_path);
                    continue;
                }
                Err(e) => {
                    println!("Failed to compress file: {}, {}", file_path, e);
                    continue;
                }
            }
        }

        zip.finish()?;
        Ok(())
    }
}
