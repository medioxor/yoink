pub mod collection {
    #[path = "collecter.rs"]
    pub mod collecter;
    #[path = "rules.rs"]
    pub mod rules;
    #[cfg(target_os = "windows")]
    #[path = "windows_reader.rs"]
    pub mod windows_reader;
}
