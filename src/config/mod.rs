/*
 * This file is part of espanso.
 *
 * Copyright (C) 2019 Federico Terzi
 *
 * espanso is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * espanso is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with espanso.  If not, see <https://www.gnu.org/licenses/>.
 */

extern crate dirs;

use std::path::{Path, PathBuf};
use std::{fs};
use crate::matcher::Match;
use std::fs::{File, create_dir_all};
use std::io::Read;
use serde::{Serialize, Deserialize};
use crate::event::KeyModifier;
use std::collections::HashSet;
use log::{error};
use std::fmt;
use std::error::Error;

pub(crate) mod runtime;

// TODO: add documentation link
const DEFAULT_CONFIG_FILE_CONTENT : &str = include_str!("../res/config.yml");

const DEFAULT_CONFIG_FILE_NAME : &str = "default.yml";
const USER_CONFIGS_FOLDER_NAME: &str = "user";
const PACKAGES_FOLDER_NAME : &str = "packages";

// Default values for primitives
fn default_name() -> String{ "default".to_owned() }
fn default_filter_title() -> String{ "".to_owned() }
fn default_filter_class() -> String{ "".to_owned() }
fn default_filter_exec() -> String{ "".to_owned() }
fn default_disabled() -> bool{ false }
fn default_log_level() -> i32 { 0 }
fn default_ipc_server_port() -> i32 { 34982 }
fn default_use_system_agent() -> bool { true }
fn default_config_caching_interval() -> i32 { 800 }
fn default_toggle_interval() -> u32 { 230 }
fn default_backspace_limit() -> i32 { 3 }
fn default_exclude_parent_matches() -> bool {false}
fn default_matches() -> Vec<Match> { Vec::new() }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Configs {
    #[serde(default = "default_name")]
    pub name: String,

    #[serde(default = "default_filter_title")]
    pub filter_title: String,

    #[serde(default = "default_filter_class")]
    pub filter_class: String,

    #[serde(default = "default_filter_exec")]
    pub filter_exec: String,

    #[serde(default = "default_disabled")]
    pub disabled: bool,

    #[serde(default = "default_log_level")]
    pub log_level: i32,

    #[serde(default = "default_ipc_server_port")]
    pub ipc_server_port: i32,

    #[serde(default = "default_use_system_agent")]
    pub use_system_agent: bool,

    #[serde(default = "default_config_caching_interval")]
    pub config_caching_interval: i32,

    #[serde(default)]
    pub toggle_key: KeyModifier,

    #[serde(default = "default_toggle_interval")]
    pub toggle_interval: u32,

    #[serde(default = "default_backspace_limit")]
    pub backspace_limit: i32,

    #[serde(default)]
    pub backend: BackendType,

    #[serde(default = "default_exclude_parent_matches")]
    pub exclude_parent_matches: bool,

    #[serde(default = "default_matches")]
    pub matches: Vec<Match>
}

// Macro used to validate config fields
#[macro_export]
macro_rules! validate_field {
    ($result:expr, $field:expr, $def_value:expr) => {
        if $field != $def_value {
            let mut field_name = stringify!($field);
            if field_name.starts_with("self.") {
                field_name = &field_name[5..];  // Remove the 'self.' prefix
            }
            error!("Validation error, parameter '{}' is reserved and can be only used in the default.yml config file", field_name);
            $result = false;
        }
    };
}

impl Configs {
    /*
     * Validate the Config instance.
     * It makes sure that user defined config instances do not define
     * attributes reserved to the default config.
     */
    fn validate_user_defined_config(&self) -> bool {
        let mut result = true;

        validate_field!(result, self.config_caching_interval, default_config_caching_interval());
        validate_field!(result, self.log_level, default_log_level());
        validate_field!(result, self.toggle_key, KeyModifier::default());
        validate_field!(result, self.toggle_interval, default_toggle_interval());
        validate_field!(result, self.backspace_limit, default_backspace_limit());
        validate_field!(result, self.ipc_server_port, default_ipc_server_port());
        validate_field!(result, self.use_system_agent, default_use_system_agent());

        result
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BackendType {
    Inject,
    Clipboard
}
impl Default for BackendType {
    fn default() -> Self {
        BackendType::Inject
    }
}

impl Configs {
    fn load_config(path: &Path) -> Result<Configs, ConfigLoadError> {
        let file_res = File::open(path);
        if let Ok(mut file) = file_res {
            let mut contents = String::new();
            let res = file.read_to_string(&mut contents);

            if let Err(_) = res {
                return Err(ConfigLoadError::UnableToReadFile)
            }

            let config_res = serde_yaml::from_str(&contents);

            match config_res {
                Ok(config) => Ok(config),
                Err(e) => {
                    Err(ConfigLoadError::InvalidYAML(path.to_owned(), e.to_string()))
                }
            }
        }else{
            Err(ConfigLoadError::FileNotFound)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigSet {
    pub default: Configs,
    pub specific: Vec<Configs>,
}

impl ConfigSet {
    pub fn load(dir_path: &Path) -> Result<ConfigSet, ConfigLoadError> {
        if !dir_path.is_dir() {
            return Err(ConfigLoadError::InvalidConfigDirectory)
        }

        // Load default configuration
        let default_file = dir_path.join(DEFAULT_CONFIG_FILE_NAME);
        let default = Configs::load_config(default_file.as_path())?;

        // Load user defined configurations

        // TODO: loading with parent merging

        let mut specific = Vec::new();

        let specific_dir = dir_path.join(USER_CONFIGS_FOLDER_NAME);
        if specific_dir.exists() {
            // Used to make sure no duplicates are present
            let mut name_set = HashSet::new();  // TODO: think about integration with packages

            let dir_entry = fs::read_dir(specific_dir);
            if dir_entry.is_err() {
                return Err(ConfigLoadError::UnableToReadFile)
            }
            let dir_entry = dir_entry.unwrap();

            for entry in dir_entry {
                let entry = entry;
                if let Ok(entry) = entry {
                    let path = entry.path();

                    // Skip non-yaml config files
                    if path.extension().unwrap_or_default().to_str().unwrap_or_default() != "yml" {
                        continue;
                    }

                    let mut config = Configs::load_config(path.as_path())?;

                    if !config.validate_user_defined_config() {
                        return Err(ConfigLoadError::InvalidParameter(path.to_owned()))
                    }

                    if config.name == "default" {
                        return Err(ConfigLoadError::MissingName(path.to_owned()));
                    }

                    if name_set.contains(&config.name) {
                        return Err(ConfigLoadError::NameDuplicate(path.to_owned()));
                    }

                    // Compute new match set, merging the parent's matches.
                    // Note: if an app-specific redefines a trigger already present in the
                    // default config, the latter gets overwritten.
                    if !config.exclude_parent_matches {
                        let mut merged_matches = config.matches.clone();
                        let mut trigger_set = HashSet::new();
                        merged_matches.iter().for_each(|m| {
                            trigger_set.insert(m.trigger.clone());
                        });
                        let parent_matches : Vec<Match> = default.matches.iter().filter(|&m| {
                            !trigger_set.contains(&m.trigger)
                        }).map(|m| m.clone()).collect();

                        merged_matches.extend(parent_matches);
                        config.matches = merged_matches;
                    }

                    // TODO: check if it contains at least a filter, and warn the user about the problem

                    name_set.insert(config.name.clone());
                    specific.push(config);
                }
            }
        }

        Ok(ConfigSet {
            default,
            specific: specific
        })
    }

    pub fn load_default() -> Result<ConfigSet, ConfigLoadError> {
        let res = dirs::home_dir();
        if let Some(home_dir) = res {
            let espanso_dir = home_dir.join(".espanso");

            // Create the espanso dir if id doesn't exist
            let res = create_dir_all(espanso_dir.as_path());

            if let Ok(_) = res {
                let default_file = espanso_dir.join(DEFAULT_CONFIG_FILE_NAME);

                // If config file does not exist, create one from template
                if !default_file.exists() {
                    let result = fs::write(&default_file, DEFAULT_CONFIG_FILE_CONTENT);
                    if result.is_err() {
                        return Err(ConfigLoadError::UnableToCreateDefaultConfig)
                    }
                }

                // Create auxiliary directories

                let user_config_dir = espanso_dir.join(USER_CONFIGS_FOLDER_NAME);
                if !user_config_dir.exists() {
                    let res = create_dir_all(user_config_dir.as_path());
                    if res.is_err() {
                        return Err(ConfigLoadError::UnableToCreateDefaultConfig)
                    }
                }

                let packages_dir = espanso_dir.join(PACKAGES_FOLDER_NAME);
                if !packages_dir.exists() {
                    let res = create_dir_all(packages_dir.as_path());
                    if res.is_err() {
                        return Err(ConfigLoadError::UnableToCreateDefaultConfig)
                    }
                }

                return ConfigSet::load(espanso_dir.as_path())
            }
        }

        return Err(ConfigLoadError::UnableToCreateDefaultConfig)
    }
}

pub trait ConfigManager<'a> {
    fn active_config(&'a self) -> &'a Configs;
    fn default_config(&'a self) -> &'a Configs;
    fn matches(&'a self) -> &'a Vec<Match>;
}

// Error handling
#[derive(Debug, PartialEq)]
pub enum ConfigLoadError {
    FileNotFound,
    UnableToReadFile,
    InvalidYAML(PathBuf, String),
    InvalidConfigDirectory,
    InvalidParameter(PathBuf),
    MissingName(PathBuf),
    NameDuplicate(PathBuf),
    UnableToCreateDefaultConfig,
}

impl fmt::Display for ConfigLoadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfigLoadError::FileNotFound =>  write!(f, "File not found"),
            ConfigLoadError::UnableToReadFile =>  write!(f, "Unable to read config file"),
            ConfigLoadError::InvalidYAML(path, e) => write!(f, "Error parsing YAML file '{}', invalid syntax: {}", path.to_str().unwrap_or_default(), e),
            ConfigLoadError::InvalidConfigDirectory =>  write!(f, "Invalid config directory"),
            ConfigLoadError::InvalidParameter(path) =>  write!(f, "Invalid parameter in '{}', use of reserved parameters in used defined configs is not permitted", path.to_str().unwrap_or_default()),
            ConfigLoadError::MissingName(path) =>  write!(f, "The 'name' field is required in user defined configurations, but it's missing in '{}'", path.to_str().unwrap_or_default()),
            ConfigLoadError::NameDuplicate(path) =>  write!(f, "Found duplicate 'name' in '{}', please use different names", path.to_str().unwrap_or_default()),
            ConfigLoadError::UnableToCreateDefaultConfig =>  write!(f, "Could not generate default config file"),
        }
    }
}

impl Error for ConfigLoadError {
    fn description(&self) -> &str {
        match self {
            ConfigLoadError::FileNotFound => "File not found",
            ConfigLoadError::UnableToReadFile => "Unable to read config file",
            ConfigLoadError::InvalidYAML(_, _) => "Error parsing YAML file, invalid syntax",
            ConfigLoadError::InvalidConfigDirectory => "Invalid config directory",
            ConfigLoadError::InvalidParameter(_) => "Invalid parameter, use of reserved parameters in user defined configs is not permitted",
            ConfigLoadError::MissingName(_) => "The 'name' field is required in user defined configurations, but it's missing",
            ConfigLoadError::NameDuplicate(_) => "Found duplicate 'name' in some configurations, please use different names",
            ConfigLoadError::UnableToCreateDefaultConfig => "Could not generate default config file",
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};
    use std::any::Any;

    const TEST_WORKING_CONFIG_FILE : &str = include_str!("../res/test/working_config.yml");
    const TEST_CONFIG_FILE_WITH_BAD_YAML : &str = include_str!("../res/test/config_with_bad_yaml.yml");

    // Test Configs

    fn create_tmp_file(string: &str) -> NamedTempFile {
        let file = NamedTempFile::new().unwrap();
        file.as_file().write_all(string.as_bytes());
        file
    }

    fn variant_eq<T>(a: &T, b: &T) -> bool {
        std::mem::discriminant(a) == std::mem::discriminant(b)
    }

    #[test]
    fn test_config_file_not_found() {
        let config = Configs::load_config(Path::new("invalid/path"));
        assert_eq!(config.is_err(), true);
        assert_eq!(config.unwrap_err(), ConfigLoadError::FileNotFound);
    }

    #[test]
    fn test_config_file_with_bad_yaml_syntax() {
        let broken_config_file = create_tmp_file(TEST_CONFIG_FILE_WITH_BAD_YAML);
        let config = Configs::load_config(broken_config_file.path());
        match config {
            Ok(_) => {assert!(false)},
            Err(e) => {
                match e {
                    ConfigLoadError::InvalidYAML(p, _) => assert_eq!(p, broken_config_file.path().to_owned()),
                    _ => assert!(false),
                }
                assert!(true);
            },
        }

    }

    #[test]
    fn test_validate_field_macro() {
        let mut result = true;

        validate_field!(result, 3, 3);
        assert_eq!(result, true);

        validate_field!(result, 10, 3);
        assert_eq!(result, false);

        validate_field!(result, 3, 3);
        assert_eq!(result, false);
    }

    #[test]
    fn test_user_defined_config_does_not_have_reserved_fields() {
        let working_config_file = create_tmp_file(r###"

        backend: Clipboard

        "###);
        let config = Configs::load_config(working_config_file.path());
        assert_eq!(config.unwrap().validate_user_defined_config(), true);
    }

    #[test]
    fn test_user_defined_config_has_reserved_fields_config_caching_interval() {
        let working_config_file = create_tmp_file(r###"

        # This should not happen in an app-specific config
        config_caching_interval: 100

        "###);
        let config = Configs::load_config(working_config_file.path());
        assert_eq!(config.unwrap().validate_user_defined_config(), false);
    }

    #[test]
    fn test_user_defined_config_has_reserved_fields_toggle_key() {
        let working_config_file = create_tmp_file(r###"

        # This should not happen in an app-specific config
        toggle_key: CTRL

        "###);
        let config = Configs::load_config(working_config_file.path());
        assert_eq!(config.unwrap().validate_user_defined_config(), false);
    }

    #[test]
    fn test_user_defined_config_has_reserved_fields_toggle_interval() {
        let working_config_file = create_tmp_file(r###"

        # This should not happen in an app-specific config
        toggle_interval: 1000

        "###);
        let config = Configs::load_config(working_config_file.path());
        assert_eq!(config.unwrap().validate_user_defined_config(), false);
    }

    #[test]
    fn test_user_defined_config_has_reserved_fields_backspace_limit() {
        let working_config_file = create_tmp_file(r###"

        # This should not happen in an app-specific config
        backspace_limit: 10

        "###);
        let config = Configs::load_config(working_config_file.path());
        assert_eq!(config.unwrap().validate_user_defined_config(), false);
    }

    #[test]
    fn test_config_loaded_correctly() {
        let working_config_file = create_tmp_file(TEST_WORKING_CONFIG_FILE);
        let config = Configs::load_config(working_config_file.path());
        assert_eq!(config.is_ok(), true);
    }

    // Test ConfigSet

    #[test]
    fn test_config_set_default_content_should_work_correctly() {
        let tmp_dir = TempDir::new().expect("unable to create temp directory");
        let default_path = tmp_dir.path().join(DEFAULT_CONFIG_FILE_NAME);
        fs::write(default_path, DEFAULT_CONFIG_FILE_CONTENT);

        let config_set = ConfigSet::load(tmp_dir.path());
        assert!(config_set.is_ok());
    }

    #[test]
    fn test_config_set_load_fail_bad_directory() {
        let config_set = ConfigSet::load(Path::new("invalid/path"));
        assert_eq!(config_set.is_err(), true);
        assert_eq!(config_set.unwrap_err(), ConfigLoadError::InvalidConfigDirectory);
    }

    #[test]
    fn test_config_set_missing_default_file() {
        let tmp_dir = TempDir::new().expect("unable to create temp directory");

        let config_set = ConfigSet::load(tmp_dir.path());
        assert_eq!(config_set.is_err(), true);
        assert_eq!(config_set.unwrap_err(), ConfigLoadError::FileNotFound);
    }

    #[test]
    fn test_config_set_invalid_yaml_syntax() {
        let tmp_dir = TempDir::new().expect("unable to create temp directory");
        let default_path = tmp_dir.path().join(DEFAULT_CONFIG_FILE_NAME);
        let default_path_copy = default_path.clone();
        fs::write(default_path, TEST_CONFIG_FILE_WITH_BAD_YAML);

        let config_set = ConfigSet::load(tmp_dir.path());
        match config_set {
            Ok(_) => {assert!(false)},
            Err(e) => {
                match e {
                    ConfigLoadError::InvalidYAML(p, _) => assert_eq!(p, default_path_copy),
                    _ => assert!(false),
                }
                assert!(true);
            },
        }
    }

    #[test]
    fn test_config_set_specific_file_with_reserved_fields() {
        let tmp_dir = TempDir::new().expect("unable to create temp directory");
        let default_path = tmp_dir.path().join(DEFAULT_CONFIG_FILE_NAME);
        fs::write(default_path, DEFAULT_CONFIG_FILE_CONTENT);

        let user_defined_path = create_user_config_file(tmp_dir.path(), "specific.yml", r###"
        config_caching_interval: 10000
        "###);
        let user_defined_path_copy = user_defined_path.clone();

        let config_set = ConfigSet::load(tmp_dir.path());
        assert!(config_set.is_err());
        assert_eq!(config_set.unwrap_err(), ConfigLoadError::InvalidParameter(user_defined_path_copy))
    }

    #[test]
    fn test_config_set_specific_file_missing_name() {
        let tmp_dir = TempDir::new().expect("unable to create temp directory");
        let default_path = tmp_dir.path().join(DEFAULT_CONFIG_FILE_NAME);
        fs::write(default_path, DEFAULT_CONFIG_FILE_CONTENT);

        let user_defined_path = create_user_config_file(tmp_dir.path(), "specific.yml", r###"
        backend: Clipboard
        "###);
        let user_defined_path_copy = user_defined_path.clone();

        let config_set = ConfigSet::load(tmp_dir.path());
        assert!(config_set.is_err());
        assert_eq!(config_set.unwrap_err(), ConfigLoadError::MissingName(user_defined_path_copy))
    }

    pub fn create_temp_espanso_directory() -> TempDir {
        let tmp_dir = TempDir::new().expect("unable to create temp directory");
        let default_path = tmp_dir.path().join(DEFAULT_CONFIG_FILE_NAME);
        fs::write(default_path, DEFAULT_CONFIG_FILE_CONTENT);

        tmp_dir
    }

    pub fn create_temp_file_in_dir(tmp_dir: &PathBuf, name: &str, content: &str) -> PathBuf {
        let user_defined_path = tmp_dir.join(name);
        let user_defined_path_copy = user_defined_path.clone();
        fs::write(user_defined_path, content);

        user_defined_path_copy
    }

    pub fn create_user_config_file(tmp_dir: &Path, name: &str, content: &str) -> PathBuf {
        let user_config_dir = tmp_dir.join(USER_CONFIGS_FOLDER_NAME);
        if !user_config_dir.exists() {
            create_dir_all(&user_config_dir);
        }

        create_temp_file_in_dir(&user_config_dir, name, content)
    }

    #[test]
    fn test_config_set_specific_file_duplicate_name() {
        let tmp_dir = create_temp_espanso_directory();

        let user_defined_path = create_user_config_file(tmp_dir.path(), "specific.yml", r###"
        name: specific1
        "###);

        let user_defined_path2 = create_user_config_file(tmp_dir.path(), "specific2.yml", r###"
        name: specific1
        "###);

        let config_set = ConfigSet::load(tmp_dir.path());
        assert!(config_set.is_err());
        assert!(variant_eq(&config_set.unwrap_err(), &ConfigLoadError::NameDuplicate(PathBuf::new())))
    }

    #[test]
    fn test_user_defined_config_set_merge_with_parent_matches() {
        let tmp_dir = TempDir::new().expect("unable to create temp directory");
        let default_path = tmp_dir.path().join(DEFAULT_CONFIG_FILE_NAME);
        fs::write(default_path, r###"
        matches:
            - trigger: ":lol"
              replace: "LOL"
            - trigger: ":yess"
              replace: "Bob"
        "###);

        let user_defined_path = create_user_config_file(tmp_dir.path(), "specific1.yml", r###"
        name: specific1

        matches:
            - trigger: "hello"
              replace: "newstring"
        "###);

        let config_set = ConfigSet::load(tmp_dir.path()).unwrap();
        assert_eq!(config_set.default.matches.len(), 2);
        assert_eq!(config_set.specific[0].matches.len(), 3);

        assert!(config_set.specific[0].matches.iter().find(|x| x.trigger == "hello").is_some());
        assert!(config_set.specific[0].matches.iter().find(|x| x.trigger == ":lol").is_some());
        assert!(config_set.specific[0].matches.iter().find(|x| x.trigger == ":yess").is_some());
    }

    #[test]
    fn test_user_defined_config_set_merge_with_parent_matches_child_priority() {
        let tmp_dir = TempDir::new().expect("unable to create temp directory");
        let default_path = tmp_dir.path().join(DEFAULT_CONFIG_FILE_NAME);
        fs::write(default_path, r###"
        matches:
            - trigger: ":lol"
              replace: "LOL"
            - trigger: ":yess"
              replace: "Bob"
        "###);

        let user_defined_path2 = create_user_config_file(tmp_dir.path(), "specific2.yml", r###"
        name: specific1

        matches:
            - trigger: ":lol"
              replace: "newstring"
        "###);

        let config_set = ConfigSet::load(tmp_dir.path()).unwrap();
        assert_eq!(config_set.default.matches.len(), 2);
        assert_eq!(config_set.specific[0].matches.len(), 2);

        assert!(config_set.specific[0].matches.iter().find(|x| x.trigger == ":lol" && x.replace == "newstring").is_some());
        assert!(config_set.specific[0].matches.iter().find(|x| x.trigger == ":yess").is_some());
    }

    #[test]
    fn test_user_defined_config_set_exclude_merge_with_parent_matches() {
        let tmp_dir = TempDir::new().expect("unable to create temp directory");
        let default_path = tmp_dir.path().join(DEFAULT_CONFIG_FILE_NAME);
        fs::write(default_path, r###"
        matches:
            - trigger: ":lol"
              replace: "LOL"
            - trigger: ":yess"
              replace: "Bob"
        "###);

        let user_defined_path2 = create_user_config_file(tmp_dir.path(), "specific2.yml", r###"
        name: specific1

        exclude_parent_matches: true

        matches:
            - trigger: "hello"
              replace: "newstring"
        "###);

        let config_set = ConfigSet::load(tmp_dir.path()).unwrap();
        assert_eq!(config_set.default.matches.len(), 2);
        assert_eq!(config_set.specific[0].matches.len(), 1);

        assert!(config_set.specific[0].matches.iter().find(|x| x.trigger == "hello" && x.replace == "newstring").is_some());
    }

    #[test]
    fn test_only_yaml_files_are_loaded_from_config() {
        let tmp_dir = TempDir::new().expect("unable to create temp directory");
        let default_path = tmp_dir.path().join(DEFAULT_CONFIG_FILE_NAME);
        fs::write(default_path, r###"
        matches:
            - trigger: ":lol"
              replace: "LOL"
            - trigger: ":yess"
              replace: "Bob"
        "###);

        let user_defined_path2 = create_user_config_file(tmp_dir.path(), "specific.zzz", r###"
        name: specific1

        exclude_parent_matches: true

        matches:
            - trigger: "hello"
              replace: "newstring"
        "###);

        let config_set = ConfigSet::load(tmp_dir.path()).unwrap();
        assert_eq!(config_set.specific.len(), 0);
    }
}