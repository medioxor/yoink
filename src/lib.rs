pub mod collection {
    #[path = "collecter.rs"]
    pub mod collecter;
    #[path = "command/collecter.rs"]
    pub mod command;
    #[path = "file/collecter.rs"]
    pub mod file;
    #[path = "memory/collecter.rs"]
    pub mod memory;
    #[path = "rules.rs"]
    pub mod rules;
    pub mod reader {
        #[cfg(target_os = "windows")]
        #[path = "../file/ntfs_reader.rs"]
        pub mod ntfs_reader;
    }
}
