mod audio;
mod config;
mod eliza;
mod openai;
mod vrchat;

use audio::AudioRecorder;
use config::Config;
use eframe::egui;
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use eliza::ElizaClient;
use openai::OpenAIClient;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use vrchat::VRChatClient;

fn main() -> eframe::Result<()> {
    // Load config
    let args: Vec<String> = std::env::args().collect();
    let mut config = Config::load();
    config.apply_args(&args);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([450.0, 600.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "Eliza Agent - VRChat Voice Chat",
        options,
        Box::new(move |cc| {
            // Setup Japanese font
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "japanese".to_owned(),
                egui::FontData::from_static(include_bytes!(
                    "../fonts/NotoSansJP-Regular.ttf"
                )),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "japanese".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "japanese".to_owned());
            cc.egui_ctx.set_fonts(fonts);

            Ok(Box::new(ElizaAgentApp::new(config)))
        }),
    )
}

#[derive(Debug, Clone, PartialEq)]
enum AppState {
    Idle,
    Monitoring,
    Recording,
    Processing,
}

enum ProcessingMessage {
    TranscriptionInProgress,
    TranscriptionComplete(String),
    VoiceCommandDetected(VoiceCommand, Option<ElizaClient>), // Voice command detected, with ElizaClient
    ElizaInProgress,
    ElizaComplete(String),
    Complete(Option<ElizaClient>), // Processing complete, return ElizaClient
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VoiceCommand {
    StartListening,
    StopListening,
}

struct ElizaAgentApp {
    state: AppState,
    config: Config,
    current_preset: String,
    status_message: String,
    recording_info: String,

    // Audio
    audio_recorder: Option<AudioRecorder>,
    audio_file_path: Option<PathBuf>,

    // Clients
    eliza_client: Option<ElizaClient>,

    // Background processing
    processing_receiver: Option<Receiver<ProcessingMessage>>,

    // Voice command state
    is_listening: bool, // Whether agent is actively listening and responding

    // Settings UI
    show_settings: bool,
    settings_openai_key: String,
    settings_agent_server_url: String,
    settings_start_threshold: f32,
    settings_silence_threshold: f32,
    settings_silence_duration: f32,
    settings_whisper_model: String,
    settings_custom_prompt: String,
    settings_agent_model: String,
    settings_max_history: usize,
    settings_system_prompt: String,
    settings_start_command_phrases: String,
    settings_stop_command_phrases: String,

    // Device management
    available_devices: Vec<String>,
    selected_device_index: usize,

    // Hotkey
    hotkey_manager: GlobalHotKeyManager,
    current_hotkey: HotKey,
    settings_hotkey: String,

    // Conversation history display
    conversation_history: Vec<(String, String)>, // (role, message)
}

impl ElizaAgentApp {
    fn new(config: Config) -> Self {
        // Get available input devices
        let mut available_devices = audio::get_input_devices().unwrap_or_else(|e| {
            eprintln!("Failed to get input devices: {}", e);
            vec![]
        });
        available_devices.insert(0, "Windows既定".to_string());

        let selected_device_index = if let Some(ref device_name) = config.input_device_name {
            available_devices
                .iter()
                .position(|d| d == device_name)
                .unwrap_or(0)
        } else {
            0
        };

        // Initialize hotkey manager
        let hotkey_manager = GlobalHotKeyManager::new().expect("Failed to create hotkey manager");
        let current_hotkey = config.parse_hotkey().unwrap_or_else(|e| {
            eprintln!("Failed to parse hotkey '{}': {}", config.hotkey, e);
            HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyG)
        });

        if let Err(e) = hotkey_manager.register(current_hotkey) {
            eprintln!("Failed to register global hotkey: {}", e);
        } else {
            println!("Global hotkey registered: {}", config.hotkey);
        }

        Self {
            state: AppState::Idle,
            current_preset: "default".to_string(),
            status_message: "Ready. Press Start to begin monitoring.".to_string(),
            recording_info: String::new(),
            audio_recorder: None,
            audio_file_path: None,
            eliza_client: None,
            processing_receiver: None,
            is_listening: false, // Start with listening disabled
            show_settings: false,
            settings_openai_key: config.openai_api_key.clone(),
            settings_agent_server_url: config.agent_server_url.clone(),
            settings_start_threshold: config.start_threshold,
            settings_silence_threshold: config.silence_threshold,
            settings_silence_duration: config.silence_duration_secs,
            settings_whisper_model: config.whisper_model.clone(),
            settings_custom_prompt: config.custom_prompt.clone(),
            settings_agent_model: config.agent_model.clone(),
            settings_max_history: config.max_length_of_conversation_history,
            settings_system_prompt: config.system_prompt.clone(),
            settings_start_command_phrases: config.start_command_phrases.clone(),
            settings_stop_command_phrases: config.stop_command_phrases.clone(),
            available_devices,
            selected_device_index,
            hotkey_manager,
            current_hotkey,
            settings_hotkey: config.hotkey.clone(),
            conversation_history: Vec::new(),
            config,
        }
    }

    fn start_monitoring(&mut self) {
        println!("Starting monitoring mode");
        self.state = AppState::Monitoring;
        let listening_status = if self.is_listening {
            "🟢 聞き取り中"
        } else {
            "🔴 待機中"
        };
        self.status_message = format!("Monitoring... {} Speak to start recording.", listening_status);

        // Initialize ElizaClient only if not already initialized
        if self.eliza_client.is_none() && !self.config.agent_server_url.is_empty() {
            println!("Creating new ElizaClient");
            self.eliza_client = Some(ElizaClient::new(
                self.config.agent_server_url.clone(),
                self.config.agent_model.clone(),
                self.config.max_length_of_conversation_history,
                self.config.system_prompt.clone(),
            ));
        } else if self.eliza_client.is_some() {
            println!("Reusing existing ElizaClient with conversation history");
        }

        // Start audio monitoring
        match AudioRecorder::new(self.config.silence_threshold) {
            Ok(mut recorder) => {
                let device_name = self
                    .config
                    .input_device_name
                    .as_ref()
                    .filter(|name| name.as_str() != "Windows既定")
                    .map(|s| s.as_str());

                match recorder.start_recording_with_device(device_name) {
                    Ok(_) => {
                        self.audio_recorder = Some(recorder);
                    }
                    Err(e) => {
                        self.status_message = format!("Error: {}", e);
                        self.state = AppState::Idle;
                    }
                }
            }
            Err(e) => {
                self.status_message = format!("Error: {}", e);
                self.state = AppState::Idle;
            }
        }
    }

    fn stop_monitoring(&mut self) {
        println!("Stopping monitoring mode");
        if let Some(mut recorder) = self.audio_recorder.take() {
            recorder.stop_recording();
        }
        self.state = AppState::Idle;
        self.status_message = "Stopped.".to_string();
        self.recording_info.clear();
    }

    fn switch_preset(&mut self, preset_name: &str) {
        println!("Switching to preset: {}", preset_name);

        // Stop monitoring if active
        if self.state != AppState::Idle {
            self.stop_monitoring();
        }

        // Load new config
        self.config = Config::load_preset(preset_name);
        self.current_preset = preset_name.to_string();

        // Update settings UI
        self.settings_openai_key = self.config.openai_api_key.clone();
        self.settings_agent_server_url = self.config.agent_server_url.clone();
        self.settings_start_threshold = self.config.start_threshold;
        self.settings_silence_threshold = self.config.silence_threshold;
        self.settings_silence_duration = self.config.silence_duration_secs;
        self.settings_whisper_model = self.config.whisper_model.clone();
        self.settings_custom_prompt = self.config.custom_prompt.clone();
        self.settings_agent_model = self.config.agent_model.clone();
        self.settings_max_history = self.config.max_length_of_conversation_history;
        self.settings_system_prompt = self.config.system_prompt.clone();

        // Update device selection
        self.selected_device_index = if let Some(ref device_name) = self.config.input_device_name {
            self.available_devices
                .iter()
                .position(|d| d == device_name)
                .unwrap_or(0)
        } else {
            0
        };

        // Clear ElizaClient to force re-initialization
        self.eliza_client = None;
        self.conversation_history.clear();

        // Update voice command settings
        self.settings_start_command_phrases = self.config.start_command_phrases.clone();
        self.settings_stop_command_phrases = self.config.stop_command_phrases.clone();

        // Reset listening state
        self.is_listening = false;

        self.status_message = format!("Switched to {}", Config::preset_display_name(preset_name));
    }

    fn start_recording(&mut self) {
        println!("Voice detected! Starting recording...");
        self.state = AppState::Recording;
        self.status_message = "Recording... Speak now!".to_string();
    }

    fn stop_recording_and_process(&mut self) {
        println!("Silence detected. Processing...");
        self.state = AppState::Processing;
        self.status_message = "Processing audio...".to_string();

        if let Some(mut recorder) = self.audio_recorder.take() {
            let audio_data = recorder.stop_recording();
            let sample_rate = recorder.get_sample_rate();

            if audio_data.is_empty() {
                self.status_message = "No audio recorded".to_string();
                self.start_monitoring();
                return;
            }

            // Save audio to WAV
            match recorder.save_audio_to_wav(&audio_data, sample_rate) {
                Ok(path) => {
                    self.audio_file_path = Some(path.clone());
                    self.start_background_processing(path);
                }
                Err(e) => {
                    self.status_message = format!("Failed to save audio: {}", e);
                    self.start_monitoring();
                }
            }
        }
    }

    fn start_background_processing(&mut self, audio_path: PathBuf) {
        let (sender, receiver) = channel();
        self.processing_receiver = Some(receiver);

        let openai_key = self.config.openai_api_key.clone();
        let whisper_model = self.config.whisper_model.clone();
        let custom_prompt = self.config.custom_prompt.clone();

        // Take ownership of eliza_client to use in the thread
        let eliza_client = self.eliza_client.take();

        // Pass voice command check config
        let config_for_commands = self.config.clone();
        let is_listening = self.is_listening;

        std::thread::spawn(move || {
            let _returned_client = process_pipeline(
                audio_path,
                openai_key,
                whisper_model,
                custom_prompt,
                eliza_client,
                sender,
                config_for_commands,
                is_listening,
            );
            // ElizaClient is returned via ProcessingMessage::Complete
        });
    }
}

fn process_pipeline(
    audio_path: PathBuf,
    openai_key: String,
    whisper_model: String,
    custom_prompt: String,
    eliza_client: Option<ElizaClient>,
    sender: Sender<ProcessingMessage>,
    config: Config,
    is_listening: bool,
) -> Option<ElizaClient> {
    // Step 1: Transcribe (always execute to detect voice commands)
    let _ = sender.send(ProcessingMessage::TranscriptionInProgress);

    let openai_client = OpenAIClient::new(openai_key, whisper_model, custom_prompt);
    let transcribed_text = match openai_client.transcribe_audio(&audio_path) {
        Ok(text) => text,
        Err(e) => {
            let _ = sender.send(ProcessingMessage::Error(format!(
                "Transcription failed: {}",
                e
            )));
            return eliza_client;
        }
    };

    let _ = sender.send(ProcessingMessage::TranscriptionComplete(
        transcribed_text.clone(),
    ));

    // Check for voice commands (but continue pipeline execution)
    let detected_command = if config.matches_start_command(&transcribed_text) {
        Some(VoiceCommand::StartListening)
    } else if config.matches_stop_command(&transcribed_text) {
        Some(VoiceCommand::StopListening)
    } else {
        None
    };

    // If not listening, skip Eliza processing
    if !is_listening && detected_command.is_none() {
        let _ = sender.send(ProcessingMessage::Complete(eliza_client));
        return None;
    }

    // Step 1.5: Send transcribed text to VRChat (as quote)
    println!("===== VRChat Sending (Transcription) =====");
    let quoted_text = format!("> {}", transcribed_text);
    let vrchat = VRChatClient::new();
    match vrchat.send_message(&quoted_text) {
        Ok(_) => {
            println!("✓ VRChat transcription sent successfully");
        }
        Err(e) => {
            eprintln!("✗ VRChat transcription send failed: {}", e);
            // Don't return error - continue to Eliza step
        }
    }

    // Step 2: Send to Eliza
    let _ = sender.send(ProcessingMessage::ElizaInProgress);

    if eliza_client.is_none() {
        let _ = sender.send(ProcessingMessage::Error("Eliza client not initialized".to_string()));
        return None;
    }

    let mut client = eliza_client.unwrap();
    let eliza_response = match client.send_message(&transcribed_text) {
        Ok(response) => response,
        Err(e) => {
            let _ = sender.send(ProcessingMessage::Error(format!("Eliza failed: {}", e)));
            return Some(client);
        }
    };

    let _ = sender.send(ProcessingMessage::ElizaComplete(eliza_response.clone()));

    // Step 3: Send to VRChat
    println!("===== VRChat Sending =====");
    println!("Response length: {} bytes, {} chars", eliza_response.len(), eliza_response.chars().count());
    let preview: String = eliza_response.chars().take(50).collect();
    println!("Response preview: {:?}...", preview);

    let vrchat = VRChatClient::new();
    match vrchat.send_message(eliza_response.as_str()) {
        Ok(_) => {
            println!("✓ VRChat message sent successfully");
        }
        Err(e) => {
            eprintln!("✗ VRChat send failed: {}", e);
            let _ = sender.send(ProcessingMessage::Error(format!("VRChat failed: {}", e)));
            return Some(client);
        }
    }

    // Send command detection message if detected, otherwise send Complete
    if let Some(command) = detected_command {
        let _ = sender.send(ProcessingMessage::VoiceCommandDetected(command, Some(client)));
        None
    } else {
        let _ = sender.send(ProcessingMessage::Complete(Some(client)));
        None
    }
}

impl eframe::App for ElizaAgentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for global hotkey events
        if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.id == self.current_hotkey.id() {
                println!("Global hotkey triggered");
                if self.state == AppState::Idle {
                    self.start_monitoring();
                } else if self.state == AppState::Monitoring {
                    self.stop_monitoring();
                }
            }
        }

        // Check for processing messages
        if let Some(receiver) = &self.processing_receiver {
            if let Ok(message) = receiver.try_recv() {
                match message {
                    ProcessingMessage::TranscriptionInProgress => {
                        self.status_message = "Transcribing audio...".to_string();
                    }
                    ProcessingMessage::TranscriptionComplete(text) => {
                        self.status_message = format!("Transcribed: {}", text);
                        if self.is_listening {
                            self.conversation_history
                                .push(("You".to_string(), text.clone()));
                        }
                    }
                    ProcessingMessage::VoiceCommandDetected(command, eliza_client) => {
                        match command {
                            VoiceCommand::StartListening => {
                                self.is_listening = true;
                                self.status_message = "🟢 聞き取り開始！".to_string();
                                println!("Voice command: Start listening");
                            }
                            VoiceCommand::StopListening => {
                                self.is_listening = false;
                                self.status_message = "🔴 聞き取り終了".to_string();
                                println!("Voice command: Stop listening");
                            }
                        }
                        self.processing_receiver = None;
                        // Restore the eliza_client for next use
                        self.eliza_client = eliza_client;
                        self.start_monitoring();
                    }
                    ProcessingMessage::ElizaInProgress => {
                        self.status_message = "Asking Eliza...".to_string();
                    }
                    ProcessingMessage::ElizaComplete(response) => {
                        self.status_message = format!("Eliza: {}", response);
                        self.conversation_history
                            .push(("Agent".to_string(), response.clone()));
                    }
                    ProcessingMessage::Complete(eliza_client) => {
                        let msg = if self.is_listening {
                            "Sent to VRChat! Ready for next input."
                        } else {
                            "Transcription complete. Waiting for start command."
                        };
                        self.status_message = msg.to_string();
                        self.processing_receiver = None;
                        // Restore the eliza_client for next use
                        self.eliza_client = eliza_client;
                        self.start_monitoring();
                    }
                    ProcessingMessage::Error(error) => {
                        self.status_message = format!("❌ Error: {}", error);
                        self.processing_receiver = None;
                        self.start_monitoring();
                    }
                }
            }
        }

        // Monitor for voice detection in Monitoring state
        if self.state == AppState::Monitoring {
            if let Some(recorder) = &self.audio_recorder {
                let amplitude = recorder.get_max_amplitude();
                if amplitude > self.config.start_threshold {
                    self.start_recording();
                }
            }
        }

        // Check for silence in Recording state
        if self.state == AppState::Recording {
            if let Some(recorder) = &self.audio_recorder {
                let buffer_size = recorder.get_buffer_size();
                let sample_rate = recorder.get_sample_rate();
                let duration = if sample_rate > 0 {
                    buffer_size as f32 / sample_rate as f32
                } else {
                    0.0
                };

                let silence_elapsed = recorder.get_silence_duration().as_secs_f32();
                self.recording_info = format!(
                    "Recording: {:.1}s | Silence: {:.1}s/{:.1}s",
                    duration, silence_elapsed, self.config.silence_duration_secs
                );

                if recorder.is_silent(self.config.silence_duration_secs) {
                    self.stop_recording_and_process();
                }
            }
            ctx.request_repaint();
        }

        // Settings modal
        if self.show_settings {
            egui::Window::new("Settings")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                        ui.label("OpenAI API Key:");
                        ui.text_edit_singleline(&mut self.settings_openai_key);
                        ui.add_space(5.0);

                        ui.label("Agent Server URL:");
                        ui.text_edit_singleline(&mut self.settings_agent_server_url);
                        ui.add_space(10.0);

                        ui.label("Start Threshold:");
                        ui.add(egui::Slider::new(&mut self.settings_start_threshold, 0.001..=0.3).logarithmic(true));
                        ui.add_space(5.0);

                        ui.label("Silence Threshold:");
                        ui.add(egui::Slider::new(&mut self.settings_silence_threshold, 0.001..=0.3).logarithmic(true));
                        ui.add_space(5.0);

                        ui.label("Silence Duration (seconds):");
                        ui.add(egui::Slider::new(&mut self.settings_silence_duration, 0.5..=10.0));
                        ui.add_space(10.0);

                        ui.label("Whisper Model:");
                        ui.text_edit_singleline(&mut self.settings_whisper_model);
                        ui.add_space(5.0);

                        ui.label("Custom Prompt:");
                        ui.add(egui::TextEdit::multiline(&mut self.settings_custom_prompt).desired_rows(2));
                        ui.add_space(10.0);

                        ui.label("Agent Model:");
                        ui.text_edit_singleline(&mut self.settings_agent_model);
                        ui.add_space(5.0);

                        ui.label("Max Conversation History:");
                        ui.add(egui::Slider::new(&mut self.settings_max_history, 1..=50));
                        ui.add_space(10.0);

                        ui.label("System Prompt:");
                        ui.add(egui::TextEdit::multiline(&mut self.settings_system_prompt).desired_rows(5));
                        ui.add_space(10.0);

                        ui.label("Start Command Phrases (separated by |):");
                        ui.text_edit_singleline(&mut self.settings_start_command_phrases);
                        ui.add_space(5.0);

                        ui.label("Stop Command Phrases (separated by |):");
                        ui.text_edit_singleline(&mut self.settings_stop_command_phrases);
                        ui.add_space(10.0);

                        ui.label("Input Device:");
                        egui::ComboBox::from_id_salt("input_device_combo")
                            .selected_text(
                                self.available_devices
                                    .get(self.selected_device_index)
                                    .unwrap_or(&"Default".to_string()),
                            )
                            .show_ui(ui, |ui| {
                                for (idx, device_name) in
                                    self.available_devices.iter().enumerate()
                                {
                                    ui.selectable_value(
                                        &mut self.selected_device_index,
                                        idx,
                                        device_name,
                                    );
                                }
                            });
                    });

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            self.config.openai_api_key = self.settings_openai_key.clone();
                            self.config.agent_server_url = self.settings_agent_server_url.clone();
                            self.config.start_threshold = self.settings_start_threshold;
                            self.config.silence_threshold = self.settings_silence_threshold;
                            self.config.silence_duration_secs = self.settings_silence_duration;
                            self.config.whisper_model = self.settings_whisper_model.clone();
                            self.config.custom_prompt = self.settings_custom_prompt.clone();
                            self.config.agent_model = self.settings_agent_model.clone();
                            self.config.max_length_of_conversation_history = self.settings_max_history;
                            self.config.system_prompt = self.settings_system_prompt.clone();
                            self.config.start_command_phrases = self.settings_start_command_phrases.clone();
                            self.config.stop_command_phrases = self.settings_stop_command_phrases.clone();

                            // Save selected input device
                            self.config.input_device_name = self
                                .available_devices
                                .get(self.selected_device_index)
                                .cloned();

                            // Save to current preset
                            match self.config.save_preset(&self.current_preset) {
                                Ok(_) => self.status_message = format!(
                                    "Settings saved to {}!",
                                    Config::preset_display_name(&self.current_preset)
                                ),
                                Err(e) => self.status_message = format!("Failed to save: {}", e),
                            }
                            self.show_settings = false;
                        }

                        if ui.button("Cancel").clicked() {
                            // Revert settings changes
                            self.settings_openai_key = self.config.openai_api_key.clone();
                            self.settings_agent_server_url = self.config.agent_server_url.clone();
                            self.settings_start_threshold = self.config.start_threshold;
                            self.settings_silence_threshold = self.config.silence_threshold;
                            self.settings_silence_duration = self.config.silence_duration_secs;
                            self.settings_whisper_model = self.config.whisper_model.clone();
                            self.settings_custom_prompt = self.config.custom_prompt.clone();
                            self.settings_agent_model = self.config.agent_model.clone();
                            self.settings_max_history = self.config.max_length_of_conversation_history;
                            self.settings_system_prompt = self.config.system_prompt.clone();
                            self.settings_start_command_phrases = self.config.start_command_phrases.clone();
                            self.settings_stop_command_phrases = self.config.stop_command_phrases.clone();

                            // Restore device index
                            self.selected_device_index =
                                if let Some(ref device_name) = self.config.input_device_name {
                                    self.available_devices
                                        .iter()
                                        .position(|d| d == device_name)
                                        .unwrap_or(0)
                                } else {
                                    0
                                };

                            self.show_settings = false;
                        }
                    });
                });
        }

        // Main UI
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);

                // Header with settings button
                ui.horizontal(|ui| {
                    ui.heading("Eliza Agent");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("⚙ Settings").clicked() {
                            self.show_settings = true;
                        }
                    });
                });

                ui.add_space(5.0);

                // Preset selector
                ui.horizontal(|ui| {
                    ui.label("設定:");
                    let current_display = Config::preset_display_name(&self.current_preset);
                    egui::ComboBox::from_id_salt("preset_selector")
                        .selected_text(&current_display)
                        .show_ui(ui, |ui| {
                            for preset in Config::list_presets() {
                                let display_name = Config::preset_display_name(&preset);
                                if ui.selectable_value(&mut self.current_preset, preset.clone(), &display_name).clicked() {
                                    self.switch_preset(&preset);
                                }
                            }
                        });
                });

                ui.add_space(10.0);

                // Listening state indicator
                let listening_text = if self.is_listening {
                    "🟢 聞き取り中 (会話モード)"
                } else {
                    "🔴 待機中 (コマンド待ち)"
                };
                let listening_color = if self.is_listening {
                    egui::Color32::from_rgb(0, 128, 0) // Dark green
                } else {
                    egui::Color32::LIGHT_RED
                };
                ui.colored_label(listening_color, listening_text);
                ui.add_space(5.0);

                // Status
                let status_color = match self.state {
                    AppState::Idle => egui::Color32::GRAY,
                    AppState::Monitoring => egui::Color32::from_rgb(0, 128, 0), // Dark green
                    AppState::Recording => egui::Color32::RED,
                    AppState::Processing => egui::Color32::from_rgb(200, 100, 0), // Dark orange
                };
                ui.colored_label(status_color, &self.status_message);

                if !self.recording_info.is_empty() {
                    ui.label(&self.recording_info);
                }

                ui.add_space(20.0);

                // Start/Stop button (always show either Start or Stop)
                let (button_text, is_stop_button) = match self.state {
                    AppState::Idle => ("▶ Start Monitoring", false),
                    AppState::Monitoring => ("⏹ Stop", true),
                    AppState::Recording => ("⏹ Stop", true),
                    AppState::Processing => ("⏹ Stop", true),
                };

                // Stop button is always enabled
                if ui
                    .add(egui::Button::new(button_text).min_size(egui::vec2(300.0, 60.0)))
                    .clicked()
                {
                    if is_stop_button {
                        self.stop_monitoring();
                    } else {
                        self.start_monitoring();
                    }
                }

                ui.add_space(20.0);

                // Conversation history
                ui.horizontal(|ui| {
                    ui.heading("Conversation");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("🗑 Clear History").clicked() {
                            self.conversation_history.clear();
                            if let Some(ref mut eliza_client) = self.eliza_client {
                                eliza_client.clear_history();
                                println!("Conversation history cleared");
                                self.status_message = "Conversation history cleared".to_string();
                            }
                        }
                    });
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (role, message) in &self.conversation_history {
                            ui.horizontal(|ui| {
                                let color = if role == "You" {
                                    egui::Color32::LIGHT_BLUE
                                } else {
                                    egui::Color32::from_rgb(0, 128, 0) // Dark green
                                };
                                ui.colored_label(color, format!("{}:", role));
                            });
                            ui.label(message);
                            ui.add_space(10.0);
                        }
                    });

                // Warning if keys not set
                if self.config.openai_api_key.is_empty() || self.config.agent_server_url.is_empty() {
                    ui.add_space(10.0);
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 165, 0),
                        "⚠ API keys or server URL not set. Please configure in Settings.",
                    );
                }
            });
        });

        // Keep updating
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}
