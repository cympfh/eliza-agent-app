use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // API Keys
    pub openai_api_key: String,

    // Audio settings
    pub start_threshold: f32,
    pub silence_threshold: f32,
    pub silence_duration_secs: f32,

    // Speech-to-Text settings
    pub whisper_model: String,
    pub custom_prompt: String,

    // Grok settings
    pub grok_http_server_url: String,
    pub grok_model: String,
    pub max_length_of_conversation_history: usize,
    pub system_prompt: String,

    // UI settings
    #[serde(default)]
    pub input_device_name: Option<String>,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
}

fn default_hotkey() -> String {
    "Ctrl+Shift+G".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            openai_api_key: String::new(),
            start_threshold: 0.02,
            silence_threshold: 0.01,
            silence_duration_secs: 2.0,
            whisper_model: "gpt-4o-transcribe".to_string(),
            custom_prompt: "A Japanese is speaking. Transcribe it.".to_string(),
            grok_http_server_url: "http://localhost:9096".to_string(),
            grok_model: "grok-4-1-fast".to_string(),
            max_length_of_conversation_history: 20,
            system_prompt: "以下は、あなたとユーザーの会話履歴です。ユーザーの発言は元は音声であり、内部的にテキスト化されたものです。ユーザーの発言は、音声認識の誤りや、文法的な不完全さを含む可能性があります。あなたは、ユーザーの発言はあたかも音声であるかのように理解し、ユーザーの意図を汲み取る必要があります。 -- 返答は20文字程度の短文である必要があります -- 言語：日本語 -- 名前：リサ -- 人格モデル：月ノ美兎 -- 職業：高校の学級委員長 -- 口調：一人称は必ず「わたくし」、基本は丁寧なですます調で話す（「ですわ」じゃなくて普通の丁寧語ね！）テンション上がると早口＆オタク丸出しの下ネタや毒舌がポロッと出る".to_string(),
            input_device_name: None,
            hotkey: default_hotkey(),
        }
    }
}

impl Config {
    /// Parse hotkey string into HotKey
    pub fn parse_hotkey(&self) -> Result<HotKey, String> {
        let parts: Vec<&str> = self.hotkey.split('+').map(|s| s.trim()).collect();

        if parts.is_empty() {
            return Err("Hotkey cannot be empty".to_string());
        }

        // Parse modifiers
        let mut modifiers = Modifiers::empty();
        for part in &parts[..parts.len() - 1] {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
                "shift" => modifiers |= Modifiers::SHIFT,
                "alt" => modifiers |= Modifiers::ALT,
                "super" | "win" | "cmd" => modifiers |= Modifiers::SUPER,
                _ => return Err(format!("Unknown modifier: {}", part)),
            }
        }

        // Parse key code
        let key_str = parts[parts.len() - 1];
        let code = match key_str.to_uppercase().as_str() {
            "A" => Code::KeyA,
            "B" => Code::KeyB,
            "C" => Code::KeyC,
            "D" => Code::KeyD,
            "E" => Code::KeyE,
            "F" => Code::KeyF,
            "G" => Code::KeyG,
            "H" => Code::KeyH,
            "I" => Code::KeyI,
            "J" => Code::KeyJ,
            "K" => Code::KeyK,
            "L" => Code::KeyL,
            "M" => Code::KeyM,
            "N" => Code::KeyN,
            "O" => Code::KeyO,
            "P" => Code::KeyP,
            "Q" => Code::KeyQ,
            "R" => Code::KeyR,
            "S" => Code::KeyS,
            "T" => Code::KeyT,
            "U" => Code::KeyU,
            "V" => Code::KeyV,
            "W" => Code::KeyW,
            "X" => Code::KeyX,
            "Y" => Code::KeyY,
            "Z" => Code::KeyZ,
            "0" => Code::Digit0,
            "1" => Code::Digit1,
            "2" => Code::Digit2,
            "3" => Code::Digit3,
            "4" => Code::Digit4,
            "5" => Code::Digit5,
            "6" => Code::Digit6,
            "7" => Code::Digit7,
            "8" => Code::Digit8,
            "9" => Code::Digit9,
            "F1" => Code::F1,
            "F2" => Code::F2,
            "F3" => Code::F3,
            "F4" => Code::F4,
            "F5" => Code::F5,
            "F6" => Code::F6,
            "F7" => Code::F7,
            "F8" => Code::F8,
            "F9" => Code::F9,
            "F10" => Code::F10,
            "F11" => Code::F11,
            "F12" => Code::F12,
            _ => return Err(format!("Unknown key: {}", key_str)),
        };

        Ok(HotKey::new(Some(modifiers), code))
    }

    /// Get the config directory
    pub fn config_dir() -> Result<PathBuf, String> {
        let config_dir = dirs::config_dir().ok_or("Failed to get config directory")?;
        let app_config_dir = config_dir.join("talk-with-grok");

        if !app_config_dir.exists() {
            fs::create_dir_all(&app_config_dir)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        Ok(app_config_dir)
    }

    /// Get the config file path for a specific preset
    pub fn config_path_for_preset(preset_name: &str) -> Result<PathBuf, String> {
        let config_dir = Self::config_dir()?;
        let filename = match preset_name {
            "default" => "config.json",
            "setting1" => "config-1.json",
            "setting2" => "config-2.json",
            "setting3" => "config-3.json",
            "setting4" => "config-4.json",
            "setting5" => "config-5.json",
            "setting6" => "config-6.json",
            "setting7" => "config-7.json",
            "setting8" => "config-8.json",
            "setting9" => "config-9.json",
            _ => return Err(format!("Unknown preset: {}", preset_name)),
        };
        Ok(config_dir.join(filename))
    }

    /// Get the default config file path
    pub fn config_path() -> Result<PathBuf, String> {
        Self::config_path_for_preset("default")
    }

    /// Get list of available presets
    pub fn list_presets() -> Vec<String> {
        vec![
            "default".to_string(),
            "setting1".to_string(),
            "setting2".to_string(),
            "setting3".to_string(),
            "setting4".to_string(),
            "setting5".to_string(),
            "setting6".to_string(),
            "setting7".to_string(),
            "setting8".to_string(),
            "setting9".to_string(),
        ]
    }

    /// Get display name for preset
    pub fn preset_display_name(preset_name: &str) -> String {
        match preset_name {
            "default" => "デフォルト設定".to_string(),
            "setting1" => "設定1".to_string(),
            "setting2" => "設定2".to_string(),
            "setting3" => "設定3".to_string(),
            "setting4" => "設定4".to_string(),
            "setting5" => "設定5".to_string(),
            "setting6" => "設定6".to_string(),
            "setting7" => "設定7".to_string(),
            "setting8" => "設定8".to_string(),
            "setting9" => "設定9".to_string(),
            _ => preset_name.to_string(),
        }
    }

    /// Load config from file
    pub fn load() -> Self {
        Self::load_preset("default")
    }

    /// Load config from a specific preset
    pub fn load_preset(preset_name: &str) -> Self {
        match Self::config_path_for_preset(preset_name) {
            Ok(path) => {
                if path.exists() {
                    match fs::read_to_string(&path) {
                        Ok(content) => match serde_json::from_str(&content) {
                            Ok(config) => {
                                println!("Config loaded from: {:?}", path);
                                return config;
                            }
                            Err(e) => {
                                eprintln!("Failed to parse config: {}", e);
                            }
                        },
                        Err(e) => {
                            eprintln!("Failed to read config file: {}", e);
                        }
                    }
                } else {
                    println!("Config file not found: {:?}, using default", path);
                }
            }
            Err(e) => {
                eprintln!("Failed to get config path: {}", e);
            }
        }

        println!("Using default config for preset: {}", preset_name);
        Self::default()
    }

    /// Save config to file
    pub fn save(&self) -> Result<(), String> {
        self.save_preset("default")
    }

    /// Save config to a specific preset
    pub fn save_preset(&self, preset_name: &str) -> Result<(), String> {
        let path = Self::config_path_for_preset(preset_name)?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        fs::write(&path, json).map_err(|e| format!("Failed to write config file: {}", e))?;
        println!("Config saved to: {:?}", path);
        Ok(())
    }

    /// Apply command line arguments
    pub fn apply_args(&mut self, args: &[String]) {
        for arg in args {
            if let Some(key) = arg.strip_prefix("--openai-api-key=") {
                self.openai_api_key = key.to_string();
                println!("OpenAI API key set from command line");
            } else if let Some(key) = arg.strip_prefix("OPENAI_API_KEY=") {
                self.openai_api_key = key.to_string();
                println!("OpenAI API key set from command line");
            }
        }
    }
}
