pub mod collection {
    #[path = "rules.rs"] pub mod rules;
    #[path = "collecter.rs"] pub mod collecter;
    #[cfg(target_os = "windows")]
    #[path = "reader_windows.rs"] pub mod reader;
}