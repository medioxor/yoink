use super::rules::CollectionRule;
use chrono::{DateTime, Local, NaiveDateTime};
use glob::glob;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::{error::Error, fs::File};
use zip::AesMode::Aes256;
use zip::ZipWriter;

#[cfg(target_os = "windows")]
use chrono::{TimeZone, Utc};
#[cfg(target_os = "windows")]
use nt_time::FileTime;
#[cfg(target_os = "windows")]
use ntfs::{
    indexes::NtfsFileNameIndex, structured_values::NtfsStandardInformation, Ntfs,
    NtfsAttributeType, NtfsFile,
};
#[cfg(target_os = "windows")]
use std::path::Path;
#[cfg(target_os = "windows")]
use windows::Win32::Storage::FileSystem::GetLogicalDriveStringsA;
#[cfg(target_os = "windows")]
mod sector_reader;
#[cfg(target_os = "windows")]
use ntfs::NtfsReadSeek;
#[cfg(target_os = "windows")]
use sector_reader::SectorReader;
#[cfg(target_os = "windows")]
use std::io;

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
    fn parse_stream(path: &str) -> (String, String) {
        if let Some(pos) = path.rfind(':') {
            let (left, right) = path.split_at(pos);
            (left.to_string(), right.to_string().replace(":", ""))
        } else {
            (path.to_string(), String::new())
        }
    }

    #[cfg(target_os = "windows")]
    pub fn collect_by_rule(rule: &CollectionRule) -> Result<Vec<String>, Box<dyn Error>> {
        let drives = Collecter::get_windows_drives()?;
        let mut files = Vec::new();
        for drive in drives {
            println!("Searching drive: {}", drive);
            let path = format!("{drive}{0}", rule.path);
            if path.chars().filter(|&c| c == ':').count() >= 2 {
                let (path, stream) = Collecter::parse_stream(path.as_str());
                println!("path: {}, stream: {}", path, stream);
                if path.contains("*") {
                    for file in &mut Collecter::search_filesystem(path.as_str())? {
                        files.push(format!("{}:{}", file, stream));
                    }
                } else {
                    files.push(format!("{}:{}", path, stream));
                }
            } else {
                if path.contains("*") {
                    files.append(&mut Collecter::search_filesystem(path.as_str())?);
                } else {
                    files.push(path);
                }
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
            if drive.len() > 0 {
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

    #[cfg(target_os = "windows")]
    pub fn compress_file_raw(
        zip: &mut ZipWriter<File>,
        file_path: String,
        encryption_key: Option<String>,
    ) -> Result<(), Box<dyn Error>> {
        let path: String;
        let stream_name: String;

        if file_path.chars().filter(|&c| c == ':').count() >= 2 {
            (path, stream_name) = Collecter::parse_stream(file_path.as_str());
        } else {
            path = file_path;
            stream_name = String::new();
        }

        let volume_path = format!("\\\\.\\{}:", path.chars().next().ok_or("Invalid path")?);

        let volume = File::open(Path::new(&volume_path))?;
        let sector_reader = SectorReader::new(volume, 4096)?;
        let mut filesystem_reader = BufReader::new(sector_reader);
        let mut ntfs = Ntfs::new(&mut filesystem_reader)?;
        ntfs.read_upcase_table(&mut filesystem_reader)?;
        let mut current_directory: Vec<NtfsFile> =
            vec![ntfs.root_directory(&mut filesystem_reader)?];

        for dir in Path::new(&path).iter().skip(1) {
            let next_dir = dir.to_str().ok_or("Invalid path")?;
            let index = current_directory
                .last()
                .unwrap()
                .directory_index(&mut filesystem_reader)?;
            let mut finder = index.finder();

            if let Some(entry) =
                NtfsFileNameIndex::find(&mut finder, &ntfs, &mut filesystem_reader, next_dir)
            {
                let file = entry?.to_file(&ntfs, &mut filesystem_reader)?;
                if !file.is_directory() {
                    let last_modified = Collecter::get_lastmodified(&file, &mut filesystem_reader)?;

                    if encryption_key.is_some() {
                        let options = zip::write::SimpleFileOptions::default()
                            .compression_method(zip::CompressionMethod::BZIP2)
                            .last_modified_time(last_modified.try_into()?)
                            .with_aes_encryption(Aes256, encryption_key.as_deref().unwrap());
                        if stream_name.is_empty() {
                            zip.start_file_from_path(path.replace(":", ""), options)?;
                        } else {
                            zip.start_file_from_path(
                                format!("{0}_{1}", path.replace(":", ""), stream_name),
                                options,
                            )?;
                        }
                    } else {
                        let options = zip::write::SimpleFileOptions::default()
                            .compression_method(zip::CompressionMethod::BZIP2)
                            .last_modified_time(last_modified.try_into()?);
                        if stream_name.is_empty() {
                            zip.start_file_from_path(path.replace(":", ""), options)?;
                        } else {
                            zip.start_file_from_path(
                                format!("{0}_{1}", path.replace(":", ""), stream_name),
                                options,
                            )?;
                        }
                    }

                    let data_item = file
                        .data(&mut filesystem_reader, &stream_name.as_str())
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::Other,
                                format!("The file does not have a stream called {}", stream_name),
                            )
                        })??;
                    let data_attribute = data_item.to_attribute()?;
                    let mut data_value = data_attribute.value(&mut filesystem_reader)?;

                    let mut buf = [0u8; 4096];

                    loop {
                        let bytes_read = data_value.read(&mut filesystem_reader, &mut buf)?;
                        if bytes_read == 0 {
                            break;
                        }
                        match zip.write_all(buf.as_ref()) {
                            Ok(_) => continue,
                            Err(_e) => break,
                        }
                    }

                    return Ok(());
                } else {
                    current_directory.push(file);
                }
            }
        }

        Err(Box::new(io::Error::new(
            io::ErrorKind::Other,
            "File not found or is a directory",
        )))
    }

    #[cfg(target_os = "windows")]
    fn get_lastmodified(
        file: &NtfsFile,
        filesystem_reader: &mut BufReader<SectorReader<File>>,
    ) -> Result<NaiveDateTime, Box<dyn Error>> {
        use std::io;

        let mut attributes = file.attributes();
        while let Some(attribute_item) = attributes.next(filesystem_reader) {
            let attribute_item = attribute_item?;
            let attribute = attribute_item.to_attribute()?;

            if let Ok(NtfsAttributeType::StandardInformation) = attribute.ty() {
                let std_info = attribute.resident_structured_value::<NtfsStandardInformation>()?;
                let file_time =
                    FileTime::from(std_info.mft_record_modification_time().nt_timestamp())
                        .to_unix_time_secs();
                let modified_timestamp = Utc
                    .timestamp_opt(file_time, 0)
                    .single()
                    .ok_or("Invalid timestamp")?;
                return Ok(modified_timestamp.naive_utc());
            }
        }

        Err(Box::new(io::Error::new(
            io::ErrorKind::Other,
            "StandardInformation attribute not found",
        )))
    }

    fn compress_file(
        reader: &mut BufReader<File>,
        zip: &mut ZipWriter<File>,
        file_path: String,
        last_modified: NaiveDateTime,
        encryption_key: Option<String>,
    ) -> Result<(), Box<dyn Error>> {
        if encryption_key.is_some() {
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::BZIP2)
                .last_modified_time(last_modified.try_into()?)
                .with_aes_encryption(Aes256, encryption_key.as_deref().unwrap());
            #[cfg(target_os = "windows")]
            zip.start_file_from_path(file_path.replace(":", ""), options)?;
            #[cfg(target_os = "linux")]
            zip.start_file_from_path(file_path, options)?;
        } else {
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::BZIP2)
                .last_modified_time(last_modified.try_into()?);
            #[cfg(target_os = "windows")]
            zip.start_file_from_path(file_path.replace(":", ""), options)?;
            #[cfg(target_os = "linux")]
            zip.start_file_from_path(file_path, options)?;
        }

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

        for file_path in &self.files {
            let file = match File::options().read(true).write(false).open(file_path) {
                Ok(file) => Some(file),
                Err(_) => None,
            };

            if file.is_none() {
                #[cfg(target_os = "windows")]
                match Collecter::compress_file_raw(
                    &mut zip,
                    file_path.to_string(),
                    self.encryption_key.clone(),
                ) {
                    Ok(_) => continue,
                    Err(e) => {
                        println!("Failed to compress file: {}, {}", file_path, e);
                        continue;
                    }
                }
                #[cfg(target_os = "linux")]
                continue;
            } else {
                let file = file.unwrap();
                let last_modify_time = file.metadata()?.modified()?;
                let mut reader = BufReader::new(file);
                let last_modify_time = DateTime::<Local>::from(last_modify_time).naive_utc();
                Collecter::compress_file(
                    &mut reader,
                    &mut zip,
                    file_path.clone(),
                    last_modify_time,
                    self.encryption_key.clone(),
                )?;
            }
        }

        zip.finish()?;
        Ok(())
    }
}
