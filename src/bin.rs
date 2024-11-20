use clap::{Parser, Subcommand};
use glob::glob;
use std::env;
use yoink::collection::collecter::Collecter;
use yoink::collection::rules::CollectionRule;

#[cfg(target_os = "windows")]
const HOSTNAME_ENV: &str = "COMPUTERNAME";
#[cfg(target_os = "linux")]
const HOSTNAME_ENV: &str = "HOSTNAME";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// collect forensic artefacts based on .yaml rules
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
        #[clap(short, long, default_value_t = format!("{0}_{1}.zip", env::var(HOSTNAME_ENV).unwrap_or("localhost".to_string()), chrono::Utc::now().timestamp_millis()))]
        /// path the to the output file, must end in .zip e.g. /path/to/output.zip
        output: String,
        /// the name of the rules to use for collection
        rules: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Collect {
            list,
            rule_dir,
            all,
            encryption_key,
            output,
            rules,
        }) => {
            if !output.ends_with(".zip") {
                print!("Output file must end in .zip, currently: {}", output);
                return;
            }
            if *list {
                let mut rules =
                    CollectionRule::get_rules_by_platform(env::consts::OS).expect("No rules found");
                println!("List of rules:");
                if !rule_dir.is_empty() {
                    glob(format!("{}/*.yaml", rule_dir).as_str())
                        .expect("Failed to find rules")
                        .for_each(|entry| match entry {
                            Ok(path) => {
                                if path.is_file() {
                                    let rule = CollectionRule::from_yaml_file(
                                        path.to_str().expect("Failed to convert path to string"),
                                    )
                                    .expect("Failed to read rule file");
                                    if rule.platform == env::consts::OS
                                        && !rules.iter().any(|r| r.name == rule.name)
                                    {
                                        rules.push(rule);
                                    }
                                }
                            }
                            Err(e) => {
                                println!("Error: {}", e);
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
                collector = Collecter::new(env::consts::OS.to_string(), None)
                    .expect("Failed to create collector");
            } else {
                collector = Collecter::new(
                    env::consts::OS.to_string(),
                    Some(encryption_key.to_string()),
                )
                .expect("Failed to create collector");
            }

            if !rule_dir.is_empty() {
                glob(format!("{}/*.yaml", rule_dir).as_str())
                    .expect("Failed to find rules")
                    .for_each(|entry| match entry {
                        Ok(path) => {
                            if path.is_file() {
                                collector
                                    .add_rule_from_file(
                                        path.to_str().expect("Failed to convert path to string"),
                                    )
                                    .expect("Failed to read rule file");
                            }
                        }
                        Err(e) => {
                            println!("Error: {}", e);
                        }
                    });
            }

            if *all {
                collector
                    .collect_all()
                    .expect("Failed to collect artefacts");

                match collector.compress_collection(output) {
                    Ok(_) => println!("Collection compressed to {}", output),
                    Err(e) => println!("{}", e),
                }
                return;
            }

            if rules.is_empty() {
                println!("No rules specified, use -l to list available rules");
                return;
            }

            for rule in rules {
                match collector.collect_by_rulename(rule) {
                    Ok(number_of_artefacts) => println!(
                        "Found {} artefacts to collect for rule: {}",
                        number_of_artefacts, rule
                    ),
                    Err(e) => println!("{}", e),
                }
            }

            match collector.compress_collection(output) {
                Ok(_) => println!("Collection compressed to {}", output),
                Err(e) => println!("{}", e),
            }
        }
        None => println!("Unsupported!"),
    }
}
