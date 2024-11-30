use super::{file::FileCollecter, memory::MemoryCollecter, rules::CollectionRule};
use chrono::NaiveDateTime;
use chrono::{DateTime, Local};
use std::io::{BufRead, BufReader, Write};
use std::{error::Error, fs::File};
use zip::{
    write::{FileOptions, SimpleFileOptions},
    AesMode::Aes256,
    CompressionMethod, ZipWriter,
};

#[cfg(target_os = "windows")]
use super::readers::ntfs_reader::{copy_file, get_lastmodified, parse_stream};

pub struct Collecter {
    encryption_key: Option<String>,
    artefacts: Vec<String>,
    file: FileCollecter,
    memory: MemoryCollecter,
}

impl Collecter {
    pub fn new(platform: String, encryption_key: Option<String>) -> Result<Self, Box<dyn Error>> {
        Ok(Collecter {
            encryption_key,
            artefacts: Vec::new(),
            file: FileCollecter::new(platform.clone())?,
            memory: MemoryCollecter::new(platform.clone())?,
        })
    }

    pub fn add_rule_from_file(&mut self, file_path: &str) -> Result<(), Box<dyn Error>> {
        let new_rule = CollectionRule::from_yaml_file(file_path)?;

        if self.file.add_rule(new_rule.clone()).is_ok() {
            return Ok(());
        }

        if self.memory.add_rule(new_rule.clone()).is_ok() {
            return Ok(());
        }

        Err("Failed to add rule".into())
    }

    pub fn collect_by_rulename(&mut self, rule_name: &str) -> Result<usize, Box<dyn Error>> {
        if let Ok(collected) = self.file.collect_by_rulename(rule_name) {
            return Ok(collected);
        }
        if let Ok(collected) = self.memory.collect_by_rulename(rule_name) {
            return Ok(collected);
        }
        Err("Failed to collect artefacts for rule".into())
    }

    pub fn collect_all(&mut self) -> Result<(), Box<dyn Error>> {
        self.file.collect_all()?;
        self.memory.collect_all()?;
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

    #[cfg(target_os = "windows")]
    fn compress_file(
        &mut self,
        zip: &mut ZipWriter<File>,
        file_path: String,
    ) -> Result<(), Box<dyn Error>> {
        use std::path::Path;

        let (path, stream_name) = parse_stream(file_path.as_str());
        let zip_path: String;

        if self.memory.get_memory_dumps().contains(&file_path) {
            zip_path = format!(
                "memory/{}",
                Path::new(&file_path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            );
        } else if stream_name.is_empty() {
            zip_path = path.replace(":", "");
        } else {
            zip_path = format!("{0}_{1}", path.replace(":", ""), stream_name);
        }

        if let Ok(last_modified) = get_lastmodified(path.clone()) {
            let options = self.get_zip_options(last_modified)?;
            zip.start_file_from_path(zip_path, options)?;
            copy_file(file_path, zip)?;
        } else {
            let file = File::options()
                .read(true)
                .write(false)
                .open(file_path.clone())?;
            let last_modified = file.metadata()?.modified()?;
            let mut reader = BufReader::new(file);
            let last_modified = DateTime::<Local>::from(last_modified).naive_utc();
            let options = self.get_zip_options(last_modified)?;

            zip.start_file_from_path(zip_path, options)?;

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
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn compress_file(
        &mut self,
        zip: &mut ZipWriter<File>,
        file_path: String,
    ) -> Result<(), Box<dyn Error>> {
        let file = File::options()
            .read(true)
            .write(false)
            .open(file_path.clone())?;
        let last_modified = file.metadata()?.modified()?;
        let mut reader = BufReader::new(file);
        let last_modified = DateTime::<Local>::from(last_modified).naive_utc();
        let options = self.get_zip_options(last_modified)?;

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
        self.artefacts.append(&mut self.file.files);
        self.artefacts.append(&mut self.memory.get_memory_dumps());

        // remove any duplicates
        let mut unique_artefacts = std::collections::HashSet::new();
        self.artefacts
            .retain(|artefact| unique_artefacts.insert(artefact.clone()));
        let unique_artefacts = self.artefacts.clone();

        if unique_artefacts.is_empty() {
            return Err("No artefacts to compress".into());
        }

        let zip_file = File::create(output_file)?;
        let mut zip: ZipWriter<File> = ZipWriter::new(zip_file);

        for artefact in unique_artefacts {
            match self.compress_file(&mut zip, artefact.clone()) {
                Ok(_) => {
                    println!("Compressed artefact: {}", artefact);
                    continue;
                }
                Err(e) => {
                    println!("Failed to compress artefact: {}, {}", artefact, e);
                    continue;
                }
            }
        }

        zip.finish()?;
        Ok(())
    }
}
