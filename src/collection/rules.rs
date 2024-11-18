use serde::{Serialize, Deserialize};
use std::fs::File;
use rust_embed::RustEmbed;
use std::error::Error;
    
#[derive(RustEmbed)]
#[folder = "rules/"]
struct RuleFile;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct CollectionRule {
    pub name: String,
    pub description: String,
    pub path: String,
    pub platform: String
}

impl CollectionRule {
    pub fn from_str(yaml: &str) -> Result<Self, Box<dyn Error>> {
        serde_yaml::from_str(yaml).map_err(|e| Box::new(e) as Box<dyn Error>)
    }

    pub fn get_rules_by_platform(platform: &str) -> Result<Vec<Self>, Box<dyn Error>> {
        Ok(CollectionRule::get_all()?.into_iter().filter(|rule| rule.platform == platform).collect())
    }

    pub fn from_yaml_file(file_path: &str) -> Result<Self, Box<dyn Error>> {
        let file = File::open(file_path)?;
        let reader = std::io::BufReader::new(file);
        serde_yaml::from_reader(reader).map_err(|e| Box::new(e) as Box<dyn Error>)
    }

    pub fn from_name(name: &str) -> Result<Self, Box<dyn Error>> {
        let rule_file = RuleFile::get(format!("{name}.yaml").as_str()).ok_or(format!("Rule {name} not found"))?;
        let file_contents = std::str::from_utf8(rule_file.data.as_ref())?;
        Self::from_str(&file_contents)
    }

    pub fn get_all() -> Result<Vec<CollectionRule>, Box<dyn Error>> {
        let mut rules = Vec::new();
        for file_path in RuleFile::iter() {
            let rule_file = RuleFile::get(file_path.as_ref()).ok_or(format!("Rule not found"))?;
            let rule_contents = std::str::from_utf8(rule_file.data.as_ref())?;
            rules.push(Self::from_str(&rule_contents)?);
        }
        Ok(rules)
    }
}