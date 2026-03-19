use serde::{Deserialize, Serialize};

/// Metadata about a screensaver plugin, returned by `--info`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub interactive: bool,
    pub key_bindings: Vec<(String, String)>,
    pub features: Vec<String>,
    pub config_fields: Vec<ConfigField>,
}

/// Describes a single configurable field for the TUI config menu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigField {
    pub key: String,
    pub label: String,
    pub kind: FieldKind,
    pub default: String,
    pub choices: Option<Vec<String>>,
}

/// The type of a config field (used by the host's config editor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldKind {
    Bool,
    U32,
    U64,
    USize,
    F64,
    Text,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_info_json_round_trip() {
        let info = PluginInfo {
            name: "test_saver".into(),
            description: "A test screensaver".into(),
            interactive: true,
            key_bindings: vec![("Space".into(), "Pause".into())],
            features: vec!["Feature A".into()],
            config_fields: vec![ConfigField {
                key: "speed".into(),
                label: "Speed".into(),
                kind: FieldKind::F64,
                default: "1.0".into(),
                choices: None,
            }],
        };
        let json = serde_json::to_string(&info).unwrap();
        let decoded: PluginInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "test_saver");
        assert!(decoded.interactive);
        assert_eq!(decoded.config_fields.len(), 1);
        assert_eq!(decoded.config_fields[0].kind, FieldKind::F64);
    }

    #[test]
    fn plugin_info_interactive_defaults_false() {
        let json =
            r#"{"name":"old","description":"","key_bindings":[],"features":[],"config_fields":[]}"#;
        let decoded: PluginInfo = serde_json::from_str(json).unwrap();
        assert!(!decoded.interactive);
    }

    #[test]
    fn config_field_with_choices() {
        let field = ConfigField {
            key: "palette".into(),
            label: "Palette".into(),
            kind: FieldKind::Text,
            default: "rainbow".into(),
            choices: Some(vec!["rainbow".into(), "mono".into(), "fire".into()]),
        };
        let json = serde_json::to_string(&field).unwrap();
        let decoded: ConfigField = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.choices.unwrap().len(), 3);
    }
}
