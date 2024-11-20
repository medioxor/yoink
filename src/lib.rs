pub mod collection {
    #[path = "rules.rs"] pub mod rules;
    #[path = "collecter.rs"] pub mod collecter;
    #[cfg(target_os = "windows")]
    #[path = "sector_reader.rs"] pub mod sector_reader;
}