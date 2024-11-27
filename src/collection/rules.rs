use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(RustEmbed)]
#[folder = "rules/"]
struct RuleFile;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryRule {
    pub name: String,
    pub description: String,
    pub platform: String,
    pub rule_type: String,
    pub process_name: String,
    pub pid: u32,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct FileRule {
    pub name: String,
    pub description: String,
    pub platform: String,
    pub rule_type: String,
    pub path: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandRule {
    pub name: String,
    pub description: String,
    pub platform: String,
    pub rule_type: String,
    pub binary: String,
    pub arguments: String,
}

pub enum CollectionRule {
    CommandRule(CommandRule),
    FileRule(FileRule),
    MemoryRule(MemoryRule),
}

impl CollectionRule {
    pub fn from_yaml_string(yaml: &str) -> Result<Self, Box<dyn Error>> {
        if let Ok(rule) = serde_yaml::from_str::<MemoryRule>(yaml) {
            return Ok(CollectionRule::MemoryRule(rule));
        }
        if let Ok(rule) = serde_yaml::from_str::<FileRule>(yaml) {
            return Ok(CollectionRule::FileRule(rule));
        }
        if let Ok(rule) = serde_yaml::from_str::<CommandRule>(yaml) {
            return Ok(CollectionRule::CommandRule(rule));
        }
        Err("Failed to parse YAML into any known rule type".into())
    }

    pub fn get_rules_by_platform(platform: &str) -> Result<Vec<Self>, Box<dyn Error>> {
        Ok(CollectionRule::get_all()?
            .into_iter()
            .filter(|rule| match rule {
                CollectionRule::CommandRule(r) => r.platform == platform,
                CollectionRule::FileRule(r) => r.platform == platform,
                CollectionRule::MemoryRule(r) => r.platform == platform,
            })
            .collect())
    }

    pub fn get_rules_by_type(rule_type: &str) -> Result<Vec<Self>, Box<dyn Error>> {
        Ok(CollectionRule::get_all()?
            .into_iter()
            .filter(|rule| match rule {
                CollectionRule::CommandRule(r) => r.rule_type == rule_type,
                CollectionRule::FileRule(r) => r.rule_type == rule_type,
                CollectionRule::MemoryRule(r) => r.rule_type == rule_type,
            })
            .collect())
    }

    pub fn get_rules_by_platform_and_type(
        platform: &str,
        rule_type: &str,
    ) -> Result<Vec<Self>, Box<dyn Error>> {
        Ok(CollectionRule::get_all()?
            .into_iter()
            .filter(|rule| match rule {
                CollectionRule::CommandRule(r) => r.platform == platform && r.rule_type == rule_type,
                CollectionRule::FileRule(r) => r.platform == platform && r.rule_type == rule_type,
                CollectionRule::MemoryRule(r) => r.platform == platform && r.rule_type == rule_type,
            })
            .collect())
    }

    pub fn from_yaml_file(file_path: &str) -> Result<Self, Box<dyn Error>> {
        let file_contents = std::fs::read_to_string(file_path)?;
        Self::from_yaml_string(&file_contents)
    }

    pub fn from_name(name: &str) -> Result<Self, Box<dyn Error>> {
        let rule_file = RuleFile::get(format!("{name}.yaml").as_str())
            .ok_or(format!("Rule {name} not found"))?;
        let file_contents = std::str::from_utf8(rule_file.data.as_ref())?;
        Self::from_yaml_string(file_contents)
    }

    pub fn get_all() -> Result<Vec<CollectionRule>, Box<dyn Error>> {
        let mut rules = Vec::new();
        for file_path in RuleFile::iter() {
            let rule_file = RuleFile::get(file_path.as_ref()).ok_or("Rule not found")?;
            let rule_contents = std::str::from_utf8(rule_file.data.as_ref())?;
            rules.push(Self::from_yaml_string(rule_contents)?);
        }
        Ok(rules)
    }
}
