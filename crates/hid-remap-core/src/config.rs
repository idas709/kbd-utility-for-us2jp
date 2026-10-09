use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read configuration file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse TOML configuration: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Failed to serialize TOML configuration: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("Validation error: {0}")]
    Validation(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutMode {
    #[default]
    Passthrough,
    #[serde(rename = "us-to-jis")]
    UsToJis,
    #[serde(rename = "jis-to-us")]
    JisToUs,
}

impl LayoutMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            LayoutMode::Passthrough => "passthrough",
            LayoutMode::UsToJis => "us-to-jis",
            LayoutMode::JisToUs => "jis-to-us",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImeMethod {
    #[default]
    #[serde(rename = "imm")]
    Imm,
    #[serde(rename = "ime_key")]
    ImeKey,
    #[serde(rename = "convert_key")]
    ConvertKey,
    #[serde(rename = "hybrid")]
    Hybrid,
}

impl ImeMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImeMethod::Imm => "imm",
            ImeMethod::ImeKey => "ime_key",
            ImeMethod::ConvertKey => "convert_key",
            ImeMethod::Hybrid => "hybrid",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_windows_layout")]
    pub windows_layout: String,
    #[serde(default = "default_true")]
    pub start_with_windows: bool,
    #[serde(default = "default_us_to_jis")]
    pub default_layout_mode: LayoutMode,
}

fn default_us_to_jis() -> LayoutMode {
    LayoutMode::UsToJis
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            windows_layout: default_windows_layout(),
            start_with_windows: true,
            default_layout_mode: LayoutMode::UsToJis,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_windows_layout() -> String {
    "jis106".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImeConfig {
    #[serde(default)]
    pub method: ImeMethod,
    #[serde(default = "default_tap_timeout_ms")]
    pub tap_timeout_ms: u32,
}

impl Default for ImeConfig {
    fn default() -> Self {
        Self {
            method: ImeMethod::default(),
            tap_timeout_ms: default_tap_timeout_ms(),
        }
    }
}

fn default_tap_timeout_ms() -> u32 {
    1000
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub instance_id: String,
    pub name: String,
    #[serde(default)]
    pub layout_mode: LayoutMode,
    #[serde(default = "default_true")]
    pub ctrl_ime_enabled: bool,
}

impl DeviceConfig {
    pub fn new(instance_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            instance_id: instance_id.into(),
            name: name.into(),
            layout_mode: LayoutMode::Passthrough,
            ctrl_ime_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub ime: ImeConfig,
    #[serde(default)]
    pub devices: Vec<DeviceConfig>,
}

impl AppConfig {
    pub fn load_from_str(toml_str: &str) -> Result<Self, ConfigError> {
        let config: AppConfig = toml::from_str(toml_str)?;
        config.validate()?;
        Ok(config)
    }

    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        Self::load_from_str(&content)
    }

    pub fn to_toml_string(&self) -> Result<String, ConfigError> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        self.validate()?;
        let content = self.to_toml_string()?;
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.ime.tap_timeout_ms == 0 {
            return Err(ConfigError::Validation(
                "ime.tap_timeout_ms must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }

    pub fn find_device(&self, instance_id: &str) -> Option<&DeviceConfig> {
        self.devices
            .iter()
            .find(|d| d.instance_id.eq_ignore_ascii_case(instance_id))
    }

    pub fn find_device_mut(&mut self, instance_id: &str) -> Option<&mut DeviceConfig> {
        self.devices
            .iter_mut()
            .find(|d| d.instance_id.eq_ignore_ascii_case(instance_id))
    }

    pub fn upsert_device(&mut self, device: DeviceConfig) {
        if let Some(existing) = self.find_device_mut(&device.instance_id) {
            *existing = device;
        } else {
            self.devices.push(device);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sample_config() {
        let sample = r#"
[general]
enabled = true
windows_layout = "jis106"
start_with_windows = true

[ime]
method = "imm"
tap_timeout_ms = 1000

[[devices]]
instance_id = 'HID\VID_1234&PID_5678\EXAMPLE_A'
name = "US Keyboard A"
layout_mode = "us-to-jis"
ctrl_ime_enabled = true

[[devices]]
instance_id = 'HID\VID_4321&PID_8765\EXAMPLE_B'
name = "JIS Keyboard B"
layout_mode = "jis-to-us"
ctrl_ime_enabled = true

[[devices]]
instance_id = 'HID\VID_1111&PID_2222\EXAMPLE_C'
name = "JIS Keyboard C"
layout_mode = "passthrough"
ctrl_ime_enabled = false
"#;

        let config = AppConfig::load_from_str(sample).expect("valid config");
        assert!(config.general.enabled);
        assert_eq!(config.general.windows_layout, "jis106");
        assert_eq!(config.ime.method, ImeMethod::Imm);
        assert_eq!(config.ime.tap_timeout_ms, 1000);
        assert_eq!(config.devices.len(), 3);

        assert_eq!(config.devices[0].name, "US Keyboard A");
        assert_eq!(config.devices[0].layout_mode, LayoutMode::UsToJis);
        assert!(config.devices[0].ctrl_ime_enabled);

        assert_eq!(config.devices[1].layout_mode, LayoutMode::JisToUs);
        assert_eq!(config.devices[2].layout_mode, LayoutMode::Passthrough);
        assert!(!config.devices[2].ctrl_ime_enabled);
    }

    #[test]
    fn test_default_config_roundtrip() {
        let mut config = AppConfig::default();
        config.upsert_device(DeviceConfig {
            instance_id: "TEST_DEV".to_string(),
            name: "Test Keyboard".to_string(),
            layout_mode: LayoutMode::UsToJis,
            ctrl_ime_enabled: true,
        });

        let toml_str = config.to_toml_string().expect("to toml");
        let parsed = AppConfig::load_from_str(&toml_str).expect("from toml");
        assert_eq!(parsed.devices.len(), 1);
        assert_eq!(parsed.devices[0].instance_id, "TEST_DEV");
    }
}

