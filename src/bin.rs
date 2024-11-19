use clap::{Parser, Subcommand};
use yoink::collection::rules::CollectionRule;
use std::env;
use yoink::collection::collecter::Collecter;
use glob::glob;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>
}

#[derive(Subcommand)]
enum Commands {
    Collect {
        #[clap(short, long, default_value_t = false)]
        /// list the rules that can be used for collection
        list: bool,
        #[clap(short, long, default_value_t = String::from(""))]
        /// supply directory with custom rules
        rule_dir: String,
        #[clap(short, long, default_value_t = false)]
        /// use all rules for collection
        all: bool,
        #[clap(short, long, default_value_t = String::from(""))]
        /// encrypt the collection with a password using AES256
        encryption_key: String,
        /// the name of the rules to use for collection
        rules: Vec<String>
    }
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Collect {
            list,
            rule_dir,
            all,
            encryption_key,
            rules,
        }) => {
            if *list {
                let mut rules = CollectionRule::get_rules_by_platform(env::consts::OS).expect("No rules found");
                println!("List of rules:");
                if !rule_dir.is_empty() {
                    glob(format!("{}/*.yaml", rule_dir).as_str()).expect("Failed to find rules").for_each(|entry| {
                        match entry {
                            Ok(path) => {
                                if path.is_file() {
                                    let rule = CollectionRule::from_yaml_file(path.to_str().expect("Failed to convert path to string")).expect("Failed to read rule file");
                                    if rule.platform == env::consts::OS && !rules.iter().any(|r| r.name == rule.name) {
                                        rules.push(rule);
                                    }
                                }
                            }
                            Err(e) => {
                                println!("Error: {}", e);
                            }
                        }
                    });
                }

                for rule in rules {
                    println!("Name: {}", rule.name);
                    println!("Description: {}", rule.description);
                    println!("Path: {}", rule.path);
                    println!();
                }
                return;
            }
            
            let mut collector: Collecter;

            if encryption_key.is_empty() {
                collector = Collecter::new(env::consts::OS.to_string(), None).expect("Failed to create collector");
            } else {
                collector = Collecter::new(env::consts::OS.to_string(), Some(encryption_key.to_string())).expect("Failed to create collector");
            }
            
            
            if !rule_dir.is_empty() {
                glob(format!("{}/*.yaml", rule_dir).as_str()).expect("Failed to find rules").for_each(|entry| {
                    match entry {
                        Ok(path) => {
                            if path.is_file() {
                                collector.add_rule_from_file(path.to_str().expect("Failed to convert path to string")).expect("Failed to read rule file");
                            }
                        }
                        Err(e) => {
                            println!("Error: {}", e);
                        }
                    }
                });
            }
            
            if *all {
                collector.collect_all().expect("Failed to collect artefacts");
                return;
            }

            for rule in rules {
                collector.collect_by_rulename(rule).expect("Failed to collect artefacts");
            }

            collector.compress_collection("collection.zip").unwrap();

        }
        None => println!("Unsupported!")
    }
}