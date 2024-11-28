use super::{file::FileCollecter, rules::CollectionRule};
use chrono::NaiveDateTime;
use std::{error::Error, fs::File};
use zip::{
    write::{FileOptions, SimpleFileOptions},
    AesMode::Aes256,
    CompressionMethod, ZipWriter,
};

#[cfg(target_os = "linux")]
use chrono::{DateTime, Local};
#[cfg(target_os = "linux")]
use std::io::{BufRead, BufReader, Write};

#[cfg(target_os = "windows")]
use super::reader::ntfs_reader::{copy_file, get_lastmodified, parse_stream};

pub struct Collecter {
    encryption_key: Option<String>,
    artefacts: Vec<String>,
    file: FileCollecter,
}

impl Collecter {
    pub fn new(platform: String, encryption_key: Option<String>) -> Result<Self, Box<dyn Error>> {
        Ok(Collecter {
            encryption_key,
            artefacts: Vec::new(),
            file: FileCollecter::new(platform)?,
        })
    }

    pub fn add_rule_from_file(&mut self, file_path: &str) -> Result<(), Box<dyn Error>> {
        let new_rule = CollectionRule::from_yaml_file(file_path)?;

        if self.file.add_rule(new_rule).is_ok() {
            return Ok(());
        }

        Err("Failed to add rule".into())
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
        if let Ok(collected) = self.file.collect_by_rulename(rule_name) {
            return Ok(collected);
        }
        Err("Failed to collect artefacts for rule".into())
    }

    pub fn collect_all(&mut self) -> Result<(), Box<dyn Error>> {
        self.file.collect_all()?;

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
    fn compress_file(
        &mut self,
        zip: &mut ZipWriter<File>,
        file_path: String,
    ) -> Result<(), Box<dyn Error>> {
        println!("Compressing file: {}", file_path);

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
        let zip_file = File::create(output_file)?;
        let mut zip: ZipWriter<File> = ZipWriter::new(zip_file);

        self.artefacts.append(&mut self.file.files);

        // remove any duplicates
        let mut unique_artefacts = std::collections::HashSet::new();
        self.artefacts
            .retain(|artefact| unique_artefacts.insert(artefact.clone()));
        let unique_artefacts = self.artefacts.clone();

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
