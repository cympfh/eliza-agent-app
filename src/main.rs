mod audio;
mod config;
mod eliza;
mod openai;
mod vrchat;

use audio::AudioRecorder;
use config::Config;
use eframe::egui;
use eliza::ElizaClient;
use openai::OpenAIClient;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use vrchat::{VRChatClient, start_mute_listener};

fn main() -> eframe::Result<()> {
    // Load config
    let args: Vec<String> = std::env::args().collect();
    let mut config = Config::load();
    config.apply_args(&args);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([450.0, 740.0])
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
    CalibratingSilence, // キャリブレーション: 無音フェーズ (2秒)
    CalibratingVoice,   // キャリブレーション: 発話フェーズ (2秒以上)
}

enum ProcessingMessage {
    TranscriptionInProgress,
    TranscriptionComplete(String),
    ElizaInProgress,
    ElizaComplete(String, bool), // response text, sleep flag
    Complete(Option<ElizaClient>), // Processing complete, return ElizaClient
    Error(String, Option<ElizaClient>), // Error with ElizaClient (to preserve history)
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

    // VRChat mute state detection
    mute_receiver: Option<Receiver<bool>>,

    // VAD: 単発ノイズスパイクで誤検出しないよう連続カウント
    voice_detection_count: u32,

    // Calibration
    calib_start_time: Option<std::time::Instant>,
    calib_rms_samples: Vec<f32>,

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
    settings_use_vrchat_mute_detection: bool,

    // Device management
    available_devices: Vec<String>,
    selected_device_index: usize,

    // Conversation history display
    conversation_history: Vec<(String, String)>, // (role, message)

    // Text input for direct text sending
    text_input: String,

    // Sleep: set to true when Eliza detects user wants to sleep
    pending_sleep: bool,
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

        // Start VRChat mute listener if enabled
        let mute_receiver = if config.use_vrchat_mute_detection {
            let (tx, rx) = channel::<bool>();
            start_mute_listener(tx);
            Some(rx)
        } else {
            None
        };

        Self {
            state: AppState::Idle,
            current_preset: "default".to_string(),
            status_message: "Ready. Press Start to begin monitoring.".to_string(),
            recording_info: String::new(),
            audio_recorder: None,
            audio_file_path: None,
            eliza_client: None,
            processing_receiver: None,
            mute_receiver,
            voice_detection_count: 0,
            calib_start_time: None,
            calib_rms_samples: Vec::new(),
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
            settings_use_vrchat_mute_detection: config.use_vrchat_mute_detection,
            available_devices,
            selected_device_index,
            conversation_history: Vec::new(),
            text_input: String::new(),
            pending_sleep: false,
            config,
        }
    }

    fn start_monitoring(&mut self) {
        println!("Starting monitoring mode");
        self.state = AppState::Monitoring;
        self.status_message = "Monitoring... Speak to start recording.".to_string();

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
        self.voice_detection_count = 0;
    }

    fn start_calibration(&mut self) {
        println!("Starting calibration: silence phase");
        // マイクを起動（silence_threshold=0 で全サンプル拾う）
        match AudioRecorder::new(0.0) {
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
                        self.state = AppState::CalibratingSilence;
                        self.calib_start_time = Some(std::time::Instant::now());
                        self.calib_rms_samples = Vec::new();
                        self.status_message = "キャリブレーション: 静かにしてください... (2秒)".to_string();
                    }
                    Err(e) => {
                        self.status_message = format!("Error: {}", e);
                    }
                }
            }
            Err(e) => {
                self.status_message = format!("Error: {}", e);
            }
        }
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
        self.settings_use_vrchat_mute_detection = self.config.use_vrchat_mute_detection;

        // Restart mute listener for new preset
        if self.config.use_vrchat_mute_detection {
            let (tx, rx) = channel::<bool>();
            start_mute_listener(tx);
            self.mute_receiver = Some(rx);
        } else {
            self.mute_receiver = None;
        }

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

        std::thread::spawn(move || {
            let _returned_client = process_pipeline(
                audio_path,
                openai_key,
                whisper_model,
                custom_prompt,
                eliza_client,
                sender,
            );
            // ElizaClient is returned via ProcessingMessage::Complete
        });
    }

    fn send_text_message(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }

        // Ensure ElizaClient is initialized
        if self.eliza_client.is_none() && !self.config.agent_server_url.is_empty() {
            self.eliza_client = Some(ElizaClient::new(
                self.config.agent_server_url.clone(),
                self.config.agent_model.clone(),
                self.config.max_length_of_conversation_history,
                self.config.system_prompt.clone(),
            ));
        }

        // Add to conversation history immediately
        self.conversation_history.push(("You".to_string(), text.clone()));
        self.status_message = "Sending to Eliza...".to_string();

        let (sender, receiver) = channel();
        self.processing_receiver = Some(receiver);

        let eliza_client = self.eliza_client.take();

        std::thread::spawn(move || {
            text_pipeline(text, eliza_client, sender);
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
) -> Option<ElizaClient> {
    // Step 1: Transcribe
    let _ = sender.send(ProcessingMessage::TranscriptionInProgress);

    let openai_client = OpenAIClient::new(openai_key, whisper_model, custom_prompt);
    let transcribed_text = match openai_client.transcribe_audio(&audio_path) {
        Ok(text) => text,
        Err(e) => {
            let _ = sender.send(ProcessingMessage::Error(
                format!("Transcription failed: {}", e),
                eliza_client,
            ));
            return None;
        }
    };

    let _ = sender.send(ProcessingMessage::TranscriptionComplete(
        transcribed_text.clone(),
    ));

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
        let _ = sender.send(ProcessingMessage::Error(
            "Eliza client not initialized".to_string(),
            None,
        ));
        return None;
    }

    let mut client = eliza_client.unwrap();
    let (eliza_response, sleep) = match client.send_message(&transcribed_text) {
        Ok(result) => result,
        Err(e) => {
            let _ = sender.send(ProcessingMessage::Error(
                format!("Eliza failed: {}", e),
                Some(client),
            ));
            return None;
        }
    };

    let _ = sender.send(ProcessingMessage::ElizaComplete(eliza_response.clone(), sleep));

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
            let _ = sender.send(ProcessingMessage::Error(
                format!("VRChat failed: {}", e),
                Some(client),
            ));
            return None;
        }
    }

    let _ = sender.send(ProcessingMessage::Complete(Some(client)));
    None
}

fn text_pipeline(
    text: String,
    eliza_client: Option<ElizaClient>,
    sender: Sender<ProcessingMessage>,
) {
    let _ = sender.send(ProcessingMessage::ElizaInProgress);

    if eliza_client.is_none() {
        let _ = sender.send(ProcessingMessage::Error(
            "Eliza client not initialized".to_string(),
            None,
        ));
        return;
    }

    // Send quoted text to VRChat
    let quoted_text = format!("> {}", text);
    let vrchat = VRChatClient::new();
    if let Err(e) = vrchat.send_message(&quoted_text) {
        eprintln!("VRChat text send failed: {}", e);
    }

    let mut client = eliza_client.unwrap();
    let (eliza_response, sleep) = match client.send_message(&text) {
        Ok(result) => result,
        Err(e) => {
            let _ = sender.send(ProcessingMessage::Error(
                format!("Eliza failed: {}", e),
                Some(client),
            ));
            return;
        }
    };

    let _ = sender.send(ProcessingMessage::ElizaComplete(eliza_response.clone(), sleep));

    let vrchat = VRChatClient::new();
    match vrchat.send_message(eliza_response.as_str()) {
        Ok(_) => {
            println!("VRChat message sent successfully");
        }
        Err(e) => {
            eprintln!("VRChat send failed: {}", e);
            let _ = sender.send(ProcessingMessage::Error(
                format!("VRChat failed: {}", e),
                Some(client),
            ));
            return;
        }
    }

    let _ = sender.send(ProcessingMessage::Complete(Some(client)));
}

impl eframe::App for ElizaAgentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for processing messages
        if let Some(receiver) = &self.processing_receiver {
            if let Ok(message) = receiver.try_recv() {
                match message {
                    ProcessingMessage::TranscriptionInProgress => {
                        self.status_message = "Transcribing audio...".to_string();
                    }
                    ProcessingMessage::TranscriptionComplete(text) => {
                        self.status_message = format!("Transcribed: {}", text);
                        self.conversation_history
                            .push(("You".to_string(), text.clone()));
                    }
                    ProcessingMessage::ElizaInProgress => {
                        self.status_message = "Asking Eliza...".to_string();
                    }
                    ProcessingMessage::ElizaComplete(response, sleep) => {
                        self.status_message = format!("Eliza: {}", response);
                        self.conversation_history
                            .push(("Agent".to_string(), response.clone()));
                        if sleep {
                            self.pending_sleep = true;
                        }
                    }
                    ProcessingMessage::Complete(eliza_client) => {
                        self.processing_receiver = None;
                        // Restore the eliza_client for next use (regardless of state)
                        self.eliza_client = eliza_client;
                        // Check if Eliza detected sleep intent
                        if self.pending_sleep {
                            self.pending_sleep = false;
                            self.stop_monitoring();
                            self.status_message = "おやすみなさい。モニタリングを停止しました。".to_string();
                        } else if self.state == AppState::Processing {
                            // Only restart monitoring if we're still in Processing state
                            // (user may have manually stopped while waiting for response)
                            self.status_message = "Sent to VRChat! Ready for next input.".to_string();
                            self.start_monitoring();
                        } else {
                            self.status_message = "Sent to VRChat!".to_string();
                        }
                    }
                    ProcessingMessage::Error(error, eliza_client) => {
                        self.processing_receiver = None;
                        // Restore ElizaClient to preserve conversation history (regardless of state)
                        if eliza_client.is_some() {
                            self.eliza_client = eliza_client;
                        }
                        // Only restart monitoring if we're still in Processing state
                        if self.state == AppState::Processing {
                            self.status_message = format!("❌ Error: {}", error);
                            self.start_monitoring();
                        }
                    }
                }
            }
        }

        // VRChat mute state detection
        if self.config.use_vrchat_mute_detection {
            if let Some(ref rx) = self.mute_receiver {
                // drain all pending messages, keep only the last
                let mut last_muted = None;
                while let Ok(is_muted) = rx.try_recv() {
                    last_muted = Some(is_muted);
                }
                if let Some(is_muted) = last_muted {
                    // MuteSelf=true → ミュート中 → start_monitoring
                    // MuteSelf=false → ミュート解除 → stop_monitoring
                    if is_muted && self.state == AppState::Idle {
                        println!("VRChat muted → start monitoring");
                        self.start_monitoring();
                    } else if !is_muted && self.state != AppState::Idle {
                        println!("VRChat unmuted → stop monitoring");
                        self.stop_monitoring();
                    }
                }
            }
        }

        // Calibration: silence phase (2 seconds)
        if self.state == AppState::CalibratingSilence {
            if let Some(recorder) = &self.audio_recorder {
                let rms = recorder.get_rms_amplitude();
                self.calib_rms_samples.push(rms);

                let elapsed = self.calib_start_time.map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);
                self.recording_info = format!("無音録音中: {:.1}s / 2.0s", elapsed);

                if elapsed >= 2.0 {
                    // 無音フェーズ完了: 最大 RMS を silence_threshold に
                    let max_rms = self.calib_rms_samples.iter().cloned().fold(0.0f32, f32::max);
                    self.config.silence_threshold = max_rms;
                    self.settings_silence_threshold = max_rms;
                    println!("Calib silence done: max_rms={:.6} → silence_threshold", max_rms);

                    // 発話フェーズへ
                    self.calib_rms_samples.clear();
                    self.calib_start_time = Some(std::time::Instant::now());
                    self.state = AppState::CalibratingVoice;
                    self.status_message = "キャリブレーション: 話してください... (2秒以上)".to_string();
                    self.recording_info = String::new();
                }
            }
            ctx.request_repaint();
        }

        // Calibration: voice phase (2+ seconds, manual stop via button)
        if self.state == AppState::CalibratingVoice {
            if let Some(recorder) = &self.audio_recorder {
                let rms = recorder.get_rms_amplitude();
                self.calib_rms_samples.push(rms);

                let elapsed = self.calib_start_time.map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);
                self.recording_info = format!("発話録音中: {:.1}s (2秒以上話したら停止ボタン)", elapsed);
            }
            ctx.request_repaint();
        }

        // Monitor for voice detection in Monitoring state
        // RMSベースで判定し、連続2回以上で録音開始 (単発ノイズスパイク誤検出防止)
        if self.state == AppState::Monitoring {
            if let Some(recorder) = &self.audio_recorder {
                let rms = recorder.get_rms_amplitude();
                if rms > self.config.start_threshold {
                    self.voice_detection_count += 1;
                    if self.voice_detection_count >= 2 {
                        self.voice_detection_count = 0;
                        self.start_recording();
                    }
                } else {
                    self.voice_detection_count = 0;
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

                        ui.checkbox(&mut self.settings_use_vrchat_mute_detection, "VRChat のミュート状態を使う");
                        ui.label("  ミュート解除で録音開始、ミュートで録音停止 (OSC 9001ポート)");
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

                            // Apply mute detection setting (restart listener if changed)
                            let mute_changed = self.config.use_vrchat_mute_detection != self.settings_use_vrchat_mute_detection;
                            self.config.use_vrchat_mute_detection = self.settings_use_vrchat_mute_detection;
                            if mute_changed {
                                if self.config.use_vrchat_mute_detection {
                                    let (tx, rx) = channel::<bool>();
                                    start_mute_listener(tx);
                                    self.mute_receiver = Some(rx);
                                } else {
                                    self.mute_receiver = None;
                                }
                            }

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
                            self.settings_use_vrchat_mute_detection = self.config.use_vrchat_mute_detection;

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

                // Status
                let status_color = match self.state {
                    AppState::Idle => egui::Color32::GRAY,
                    AppState::Monitoring => egui::Color32::from_rgb(0, 128, 0),
                    AppState::Recording => egui::Color32::RED,
                    AppState::Processing => egui::Color32::from_rgb(200, 100, 0),
                    AppState::CalibratingSilence => egui::Color32::from_rgb(100, 100, 200),
                    AppState::CalibratingVoice => egui::Color32::from_rgb(200, 100, 200),
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
                    AppState::CalibratingSilence => ("⏹ Stop", true),
                    AppState::CalibratingVoice => ("⏹ 停止して閾値を確定", true),
                };

                // Calculate silence progress for Recording state
                let silence_progress = if self.state == AppState::Recording {
                    if let Some(recorder) = &self.audio_recorder {
                        let silence_elapsed = recorder.get_silence_duration().as_secs_f32();
                        (silence_elapsed / self.config.silence_duration_secs).min(1.0)
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

                let button_size = egui::vec2(300.0, 60.0);
                let (rect, response) = ui.allocate_exact_size(button_size, egui::Sense::click());

                // Draw button background
                let visuals = ui.style().interact(&response);
                ui.painter().rect_filled(rect, visuals.rounding, visuals.bg_fill);

                // Draw silence progress bar (start full, drain as silence progresses) when recording
                if self.state == AppState::Recording {
                    let fill_height = rect.height() * (1.0 - silence_progress);
                    if fill_height > 0.0 {
                        let progress_rect = egui::Rect::from_min_size(
                            egui::pos2(rect.min.x, rect.max.y - fill_height),
                            egui::vec2(rect.width(), fill_height),
                        );
                        ui.painter().rect_filled(
                            progress_rect,
                            visuals.rounding,
                            egui::Color32::from_rgb(100, 200, 255),
                        );
                    }
                }

                // Draw button border
                ui.painter().rect_stroke(rect, visuals.rounding, visuals.bg_stroke);

                // Draw button text
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    button_text,
                    egui::FontId::proportional(18.0),
                    visuals.text_color(),
                );

                if response.clicked() {
                    if is_stop_button {
                        if self.state == AppState::CalibratingVoice {
                            // 発話フェーズ完了: 平均 RMS を start_threshold に
                            if !self.calib_rms_samples.is_empty() {
                                let avg_rms = self.calib_rms_samples.iter().sum::<f32>()
                                    / self.calib_rms_samples.len() as f32;
                                self.config.start_threshold = avg_rms;
                                self.settings_start_threshold = avg_rms;
                                println!("Calib voice done: avg_rms={:.6} → start_threshold", avg_rms);
                                self.status_message = format!(
                                    "✓ キャリブレーション完了! silence={:.4}, start={:.4}",
                                    self.config.silence_threshold,
                                    self.config.start_threshold
                                );
                            }
                            if let Some(mut recorder) = self.audio_recorder.take() {
                                recorder.stop_recording();
                            }
                            self.state = AppState::Idle;
                            self.recording_info.clear();
                            self.calib_rms_samples.clear();
                            self.calib_start_time = None;
                        } else {
                            self.stop_monitoring();
                        }
                    } else {
                        self.start_monitoring();
                    }
                }

                ui.add_space(5.0);

                // Calibration button (Idle 時のみ表示)
                if self.state == AppState::Idle {
                    if ui
                        .add(egui::Button::new("⚙ 音量閾値を自動設定").min_size(egui::vec2(300.0, 30.0)))
                        .clicked()
                    {
                        self.start_calibration();
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
                                // Save memory before clearing
                                if let Err(e) = eliza_client.save_memory() {
                                    eprintln!("Failed to save memory on clear: {}", e);
                                }
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
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for (role, message) in &self.conversation_history {
                            ui.horizontal(|ui| {
                                let color = if role == "You" {
                                    egui::Color32::from_rgb(30, 80, 180)
                                } else {
                                    egui::Color32::from_rgb(0, 128, 0) // Dark green
                                };
                                ui.colored_label(color, format!("{}:", role));
                            });
                            ui.label(message);
                            ui.add_space(10.0);
                        }
                    });

                // Text input area
                ui.add_space(10.0);
                ui.separator();
                ui.label("テキスト送信 (Ctrl+Enter で送信 / Shift+Enter で改行):");

                let text_edit = egui::TextEdit::multiline(&mut self.text_input)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text("ここにテキストを入力...");

                let response = ui.add(text_edit);

                // Handle Ctrl+Enter to send
                if response.has_focus() {
                    let ctrl = ctx.input(|i| i.modifiers.ctrl);
                    let shift = ctx.input(|i| i.modifiers.shift);
                    let enter_pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter));

                    if enter_pressed && ctrl && !shift {
                        // Ctrl+Enter: send
                        let text = self.text_input.trim().to_string();
                        if !text.is_empty() && self.processing_receiver.is_none() {
                            self.text_input.clear();
                            self.send_text_message(text);
                        }
                    } else if enter_pressed && !shift && !ctrl {
                        // Plain Enter: do nothing (remove the newline that was just added)
                        // Remove trailing newline if added by egui
                        if self.text_input.ends_with('\n') {
                            self.text_input.pop();
                        }
                    }
                    // Shift+Enter: allow newline (default egui behavior)
                }

                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    let send_enabled = !self.text_input.trim().is_empty()
                        && self.processing_receiver.is_none();
                    if ui
                        .add_enabled(send_enabled, egui::Button::new("送信 (Ctrl+Enter)"))
                        .clicked()
                    {
                        let text = self.text_input.trim().to_string();
                        if !text.is_empty() {
                            self.text_input.clear();
                            self.send_text_message(text);
                        }
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

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        println!("App exiting, saving memory...");
        if let Some(ref eliza_client) = self.eliza_client {
            if let Err(e) = eliza_client.save_memory() {
                eprintln!("Failed to save memory on exit: {}", e);
            }
        }
    }
}
