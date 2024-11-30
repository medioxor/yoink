use super::rules::CollectionRule;
use super::rules::FileRule;
use ignore::{WalkBuilder, WalkState};
use num_cpus;
use regex::Regex;
use std::{
    cmp,
    sync::mpsc::{self, Sender},
};
use std::{env, error::Error};

#[cfg(target_os = "windows")]
use super::readers::ntfs_reader::{does_file_exist, parse_stream};
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

    fn search_filesystem(
        depth: usize,
        path: String,
        patterns: Vec<String>,
    ) -> Result<Vec<String>, Box<dyn Error>> {
        let mut builder = WalkBuilder::new(path);
        let walker = builder
            .hidden(true)
            .max_depth(Some(depth))
            .follow_links(true)
            .same_file_system(false)
            .ignore(false)
            .skip_stdout(true)
            .git_ignore(false)
            .threads(cmp::min(12, num_cpus::get()));

        let (tx, rx) = mpsc::channel::<String>();
        walker.build_parallel().run(|| {
            let tx: Sender<String> = tx.clone();
            let patterns = patterns.clone();
            Box::new({
                move |path_entry| {
                    if let Ok(entry) = path_entry {
                        if entry.clone().into_path().is_dir() {
                            return WalkState::Continue;
                        }
                        let path = entry.path().to_string_lossy().to_string();
                        let file = entry
                            .path()
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        for pattern in patterns.iter() {
                            #[cfg(target_os = "windows")]
                            let (pattern, stream) = parse_stream(pattern);
                            if let Ok(regex) = Regex::new(pattern) {
                                if regex.is_match(&path) || regex.is_match(&file) {
                                    #[cfg(target_os = "windows")]
                                    tx.send(format!("{0}:{1}", path.clone(), stream))
                                        .unwrap_or_default();
                                    #[cfg(target_os = "linux")]
                                    tx.send(path.clone()).unwrap_or_default();
                                    return WalkState::Continue;
                                }
                            }
                        }
                        return WalkState::Continue;
                    }
                    WalkState::Continue
                }
            })
        });
        let stdout_thread = std::thread::spawn(move || {
            let mut found: Vec<String> = Vec::new();
            for path in rx {
                found.push(path);
            }
            found
        });
        drop(tx);
        Ok(stdout_thread.join().unwrap_or_default())
    }

    #[cfg(target_os = "linux")]
    pub fn collect_by_rule(rule: &FileRule) -> Result<Vec<String>, Box<dyn Error>> {
        let mut files: Vec<String> = Vec::new();
        for path in rule.paths.clone() {
            if std::path::Path::new(&path).exists() {
                files.push(path.clone());
            }
            for file in &mut FileCollecter::search_filesystem(
                rule.recursion_depth,
                "/".to_string(),
                rule.paths.clone(),
            )? {
                files.push(file.clone());
            }
        }
        Ok(files)
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
        for drive_letter in drives {
            println!("Searching drive: {}", drive_letter);
            for path in rule.paths.clone() {
                let (mut file_path, stream) = parse_stream(&path);
                if file_path.contains(":") {
                    file_path = file_path
                        .trim_start_matches(|c: char| c.is_ascii_alphabetic())
                        .trim_start_matches(":")
                        .to_string();
                }
                if does_file_exist(drive_letter.clone(), file_path.clone()).unwrap_or(false) {
                    files.push(format!(
                        "{0}:\\{1}:{2}",
                        drive_letter.clone(),
                        file_path,
                        stream
                    ));
                }
            }
            for file in &mut FileCollecter::search_filesystem(
                rule.recursion_depth,
                format!("{}:\\", drive_letter),
                rule.paths.clone(),
            )? {
                files.push(file.clone());
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
                drives.push(drive.to_string().replace(":\\", ""));
            }
        }
        Ok(drives)
    }

    pub fn collect_all(&mut self) -> Result<(), Box<dyn Error>> {
        for rule in &self.rules {
            match FileCollecter::collect_by_rule(rule) {
                Ok(mut files) => {
                    self.files.append(&mut files);
                    println!(
                        "Collected {0} artefacts for rule: {1}",
                        self.files.len(),
                        rule.name
                    );
                }
                Err(e) => println!("Failed to collect artefacts for rule: {}\n{}", rule.name, e),
            }
        }
        Ok(())
    }
}
