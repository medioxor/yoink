use super::rules::CollectionRule;
use std::{env, error::Error};

use super::rules::MemoryRule;

#[cfg(target_os = "windows")]
use minidump_writer::minidump_writer::MinidumpWriter;
use minidump_writer::MinidumpType;
use windows::Win32::System::ProcessStatus::EnumProcesses;
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
use windows::Win32::System::ProcessStatus::GetModuleBaseNameA;

pub struct MemoryCollecter {
    rules: Vec<MemoryRule>,
    memory_dumps: Vec<String>,
}

pub struct Process {
    pub name: String,
    pub pid: u32,
}

impl Drop for MemoryCollecter {
    fn drop(&mut self) {
        for memory_dump in &self.memory_dumps {
            if let Err(e) = std::fs::remove_file(memory_dump) {
                println!("Failed to remove memory dump: {0}, {1}", memory_dump, e);
            }
        }
    }
}

impl MemoryCollecter {
    pub fn new(platform: String) -> Result<Self, Box<dyn Error>> {
        Ok(MemoryCollecter {
            rules: CollectionRule::get_rules_by_platform_and_type(platform.as_str(), "memory")?
                .into_iter()
                .filter_map(|rule| {
                    if let CollectionRule::MemoryRule(rule) = rule {
                        Some(rule)
                    } else {
                        None
                    }
                })
                .collect(),
            memory_dumps: Vec::new(),
        })
    }

    pub fn get_memory_dumps(&self) -> Vec<String> {
        self.memory_dumps.clone()
    }

    pub fn add_rule(&mut self, new_rule: CollectionRule) -> Result<(), Box<dyn Error>> {
        if let CollectionRule::MemoryRule(rule) = new_rule {
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

    #[cfg(target_os = "linux")]
    pub fn collect_by_rule(rule: &MemoryRule) -> Result<Vec<String>, Box<dyn Error>> {
        todo!()
    }

    pub fn collect_by_rulename(&mut self, rule_name: &str) -> Result<usize, Box<dyn Error>> {
        let rule = self
            .rules
            .iter()
            .find(|rule| rule.name == rule_name)
            .ok_or_else(|| format!("Rule with name '{}' not found", rule_name))?;
        let mut memory_dumps = MemoryCollecter::collect_by_rule(rule)?;
        let memory_dumps_len = memory_dumps.len();
        self.memory_dumps.append(&mut memory_dumps);
        Ok(memory_dumps_len)
    }

    #[cfg(target_os = "windows")]
    pub fn get_processes() -> Result<Vec<Process>, Box<dyn Error>> {
        let mut processes: Vec<Process> = Vec::new();
        let mut process_ids: Vec<u32> = vec![0; 20000];
        let mut bytes_returned: u32 = 0;

        loop {
            unsafe {
                match EnumProcesses(
                    process_ids.as_mut_ptr(),
                    (process_ids.len() * std::mem::size_of::<u32>()) as u32,
                    &mut bytes_returned, 
                ) {
                    Ok(_) => break,
                    Err(_) => {
                        if bytes_returned == 0 {
                            return Err("Failed to enumerate processes".into());
                        }
                        process_ids = vec![0; process_ids.len() * 2];
                    }
                }
            }
        }

        let num_processes = bytes_returned as usize / std::mem::size_of::<u32>();

        for &pid in &process_ids[0..num_processes] {
            if pid == 0 { continue; }

            if let Ok(handle) = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) } {
                let mut name = [0u8; 1024];
                unsafe {
                    GetModuleBaseNameA(handle, None, &mut name);
                }
                let name = String::from_utf8_lossy(&name).trim_matches(char::from(0)).to_string();
                processes.push(Process { name, pid });
            }
        }

        Ok(processes)
    }

    #[cfg(target_os = "windows")]
    pub fn collect_by_rule(rule: &MemoryRule) -> Result<Vec<String>, Box<dyn Error>> {
        let mut memory_dumps = Vec::new();
        let processes = MemoryCollecter::get_processes()?;

        for process in processes {
            if process.name.to_ascii_lowercase() == rule.process_name.to_ascii_lowercase() || process.pid == rule.pid {
                println!("Found process: {0} with pid: {1}, dumping memory", process.name, process.pid);
                let file_name = format!("{0}_{1}.dmp", process.name, chrono::Utc::now().timestamp_millis());
                let mut minidump_file = match std::fs::File::create(&file_name) {
                    Ok(file) => file,
                    Err(e) => {
                        println!("Failed to create dump file: {0}, {1}", file_name, e);
                        continue;
                    }
                };

                let mindump_file_full_path = std::fs::canonicalize(&file_name)?.to_str().unwrap_or(file_name.as_str()).to_string().replace("\\\\?\\", "");

                let crash_context = crash_context::CrashContext {
                    exception_pointers: std::ptr::null(),
                    process_id: process.pid,
                    thread_id: 0,
                    exception_code: 0,
                };

                let minidump_type = MinidumpType::WithFullMemory | MinidumpType::WithHandleData | MinidumpType::WithModuleHeaders | MinidumpType::WithUnloadedModules | MinidumpType::WithProcessThreadData | MinidumpType::WithFullMemoryInfo | MinidumpType::WithThreadInfo;

                match MinidumpWriter::dump_crash_context(crash_context, Some(minidump_type), &mut minidump_file) {
                    Ok(_) => {
                        memory_dumps.push(mindump_file_full_path);
                    }
                    Err(e) => {
                        println!("Failed to dump memory for process: {0}, {1}", process.name, e);
                    }
                }
            }
        }
        
        Ok(memory_dumps)
    }

    pub fn collect_all(&mut self) -> Result<(), Box<dyn Error>> {
        for rule in &self.rules {
            match MemoryCollecter::collect_by_rule(rule) {
                Ok(mut memory_dumps) => {
                    self.memory_dumps.append(&mut memory_dumps);
                    println!(
                        "Collected {0} artefacts for rule: {1}",
                        self.memory_dumps.len(),
                        rule.name
                    );
                }
                Err(e) => println!("Failed to collect artefacts for rule: {}\n{}", rule.name, e),
            }
        }
        Ok(())
    }
}