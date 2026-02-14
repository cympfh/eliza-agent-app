use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct AudioRecorder {
    audio_buffer: Arc<Mutex<Vec<f32>>>,
    stream: Option<cpal::Stream>,
    sample_rate: u32,
    last_sound_time: Arc<Mutex<Instant>>,
    silence_threshold: f32,
    recording_start_time: Arc<Mutex<Option<Instant>>>,
    current_max_amplitude: Arc<Mutex<f32>>,
}

impl AudioRecorder {
    pub fn new(silence_threshold: f32) -> Result<Self, String> {
        Ok(Self {
            audio_buffer: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            sample_rate: 0,
            last_sound_time: Arc::new(Mutex::new(Instant::now())),
            silence_threshold,
            recording_start_time: Arc::new(Mutex::new(None)),
            current_max_amplitude: Arc::new(Mutex::new(0.0)),
        })
    }

    pub fn get_max_amplitude(&self) -> f32 {
        *self.current_max_amplitude.lock().unwrap()
    }

    pub fn is_silent(&self, silence_duration_secs: f32) -> bool {
        // Check if we're still in the grace period (3 seconds after recording starts)
        let start_time = self.recording_start_time.lock().unwrap();
        if let Some(start) = *start_time {
            let elapsed = start.elapsed();
            if elapsed < Duration::from_secs(3) {
                // Still in grace period, not silent
                if elapsed.as_secs() % 10 == 0 {
                    println!("Grace period: {:.1}s / 3.0s", elapsed.as_secs_f32());
                }
                return false;
            }
        }

        let last_sound = self.last_sound_time.lock().unwrap();
        let silence_duration = last_sound.elapsed();
        let is_silent = silence_duration >= Duration::from_secs_f32(silence_duration_secs);

        if is_silent {
            println!(
                "SILENT DETECTED: {:.1}s >= {:.1}s",
                silence_duration.as_secs_f32(),
                silence_duration_secs
            );
        }

        is_silent
    }

    pub fn get_silence_duration(&self) -> Duration {
        let last_sound = self.last_sound_time.lock().unwrap();
        last_sound.elapsed()
    }

    pub fn reset_silence_timer(&self) {
        let mut last_sound = self.last_sound_time.lock().unwrap();
        *last_sound = Instant::now();
    }

    pub fn start_recording_with_device(&mut self, device_name: Option<&str>) -> Result<(), String> {
        // Get the default host
        let host = cpal::default_host();

        // Get the input device
        let device = if let Some(name) = device_name {
            // Find device by name
            host.input_devices()
                .map_err(|e| format!("Failed to get input devices: {}", e))?
                .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                .ok_or(format!("Input device '{}' not found", name))?
        } else {
            // Use default device
            host.default_input_device()
                .ok_or("No input device available")?
        };

        println!("Using input device: {}", device.name().unwrap_or_default());

        // Get the default input config
        let default_config = device
            .default_input_config()
            .map_err(|e| format!("Failed to get default input config: {}", e))?;

        // Try to force mono (1 channel) recording first
        let mono_config = cpal::StreamConfig {
            channels: 1,
            sample_rate: default_config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        // Clear previous buffer and reset silence timer
        {
            let mut buffer = self.audio_buffer.lock().unwrap();
            buffer.clear();
        }
        self.reset_silence_timer();

        // Reset max amplitude
        {
            let mut max_amp = self.current_max_amplitude.lock().unwrap();
            *max_amp = 0.0;
        }

        // Set recording start time for grace period
        {
            let mut start_time = self.recording_start_time.lock().unwrap();
            *start_time = Some(Instant::now());
        }

        // Create the input stream
        let buffer_clone = Arc::clone(&self.audio_buffer);
        let last_sound_clone = Arc::clone(&self.last_sound_time);
        let max_amplitude_clone = Arc::clone(&self.current_max_amplitude);
        let threshold = self.silence_threshold;

        // First try with mono config
        let stream_result = match default_config.sample_format() {
            cpal::SampleFormat::F32 => self.build_input_stream::<f32>(
                &device,
                &mono_config,
                buffer_clone.clone(),
                last_sound_clone.clone(),
                max_amplitude_clone.clone(),
                threshold,
            ),
            cpal::SampleFormat::I16 => self.build_input_stream::<i16>(
                &device,
                &mono_config,
                buffer_clone.clone(),
                last_sound_clone.clone(),
                max_amplitude_clone.clone(),
                threshold,
            ),
            cpal::SampleFormat::U16 => self.build_input_stream::<u16>(
                &device,
                &mono_config,
                buffer_clone.clone(),
                last_sound_clone.clone(),
                max_amplitude_clone.clone(),
                threshold,
            ),
            _ => return Err("Unsupported sample format".to_string()),
        };

        let (stream, _actual_channels) = match stream_result {
            Ok(stream) => {
                self.sample_rate = mono_config.sample_rate.0;
                println!(
                    "Sample rate: {}Hz, Channels: {} (forced mono), Format: {:?}",
                    self.sample_rate,
                    mono_config.channels,
                    default_config.sample_format()
                );
                (stream, 1)
            }
            Err(e) => {
                // Mono config failed, fall back to default config
                println!(
                    "Mono config not supported ({}), falling back to default config",
                    e
                );
                let default_stream_config = default_config.config();
                self.sample_rate = default_stream_config.sample_rate.0;
                let channels = default_stream_config.channels;

                println!(
                    "Sample rate: {}Hz, Channels: {} (using default), Format: {:?}",
                    self.sample_rate,
                    channels,
                    default_config.sample_format()
                );

                let stream = match default_config.sample_format() {
                    cpal::SampleFormat::F32 => self.build_input_stream_with_channels::<f32>(
                        &device,
                        &default_stream_config,
                        buffer_clone,
                        last_sound_clone,
                        max_amplitude_clone,
                        threshold,
                        channels,
                    ),
                    cpal::SampleFormat::I16 => self.build_input_stream_with_channels::<i16>(
                        &device,
                        &default_stream_config,
                        buffer_clone,
                        last_sound_clone,
                        max_amplitude_clone,
                        threshold,
                        channels,
                    ),
                    cpal::SampleFormat::U16 => self.build_input_stream_with_channels::<u16>(
                        &device,
                        &default_stream_config,
                        buffer_clone,
                        last_sound_clone,
                        max_amplitude_clone,
                        threshold,
                        channels,
                    ),
                    _ => return Err("Unsupported sample format".to_string()),
                }?;

                (stream, channels)
            }
        };

        stream
            .play()
            .map_err(|e| format!("Failed to play stream: {}", e))?;
        self.stream = Some(stream);

        Ok(())
    }

    pub fn stop_recording(&mut self) -> Vec<f32> {
        // Stop and drop the stream
        self.stream = None;

        // Return the recorded audio data
        let buffer = self.audio_buffer.lock().unwrap();
        buffer.clone()
    }

    pub fn get_buffer_size(&self) -> usize {
        let buffer = self.audio_buffer.lock().unwrap();
        buffer.len()
    }

    pub fn get_sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn build_input_stream<T>(
        &self,
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        buffer: Arc<Mutex<Vec<f32>>>,
        last_sound_time: Arc<Mutex<Instant>>,
        current_max_amplitude: Arc<Mutex<f32>>,
        threshold: f32,
    ) -> Result<cpal::Stream, String>
    where
        T: cpal::Sample + cpal::SizedSample,
        f32: cpal::FromSample<T>,
    {
        let err_fn = |err| eprintln!("An error occurred on the audio stream: {}", err);

        let stream = device
            .build_input_stream(
                config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    let mut buffer = buffer.lock().unwrap();
                    let mut has_sound = false;
                    let mut max_amplitude = 0.0f32;

                    for &sample in data.iter() {
                        let sample_f32: f32 = cpal::Sample::from_sample(sample);
                        buffer.push(sample_f32);

                        let abs_sample = sample_f32.abs();
                        max_amplitude = max_amplitude.max(abs_sample);

                        // Check if this sample exceeds the silence threshold
                        if abs_sample > threshold {
                            has_sound = true;
                        }
                    }

                    // Update current max amplitude (exponential moving average for smooth display)
                    {
                        let mut current_max = current_max_amplitude.lock().unwrap();
                        *current_max = current_max.max(max_amplitude) * 0.95; // Decay slowly
                    }

                    // Debug: Print max amplitude every 50 buffers (~1 second)
                    static mut BUFFER_COUNT: u32 = 0;
                    unsafe {
                        BUFFER_COUNT += 1;
                        if BUFFER_COUNT.is_multiple_of(50) {
                            println!(
                                "Max amplitude: {:.6}, Has sound: {}, Threshold: {}",
                                max_amplitude, has_sound, threshold
                            );
                        }
                    }

                    // Update last sound time if sound was detected
                    if has_sound {
                        let mut last_sound = last_sound_time.lock().unwrap();
                        *last_sound = Instant::now();
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| format!("Failed to build input stream: {}", e))?;

        Ok(stream)
    }

    fn build_input_stream_with_channels<T>(
        &self,
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        buffer: Arc<Mutex<Vec<f32>>>,
        last_sound_time: Arc<Mutex<Instant>>,
        current_max_amplitude: Arc<Mutex<f32>>,
        threshold: f32,
        channels: u16,
    ) -> Result<cpal::Stream, String>
    where
        T: cpal::Sample + cpal::SizedSample,
        f32: cpal::FromSample<T>,
    {
        let err_fn = |err| eprintln!("An error occurred on the audio stream: {}", err);

        let stream = device
            .build_input_stream(
                config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    let mut buffer = buffer.lock().unwrap();
                    let mut has_sound = false;
                    let mut max_amplitude = 0.0f32;

                    if channels == 1 {
                        // Mono: process all samples directly
                        for &sample in data.iter() {
                            let sample_f32: f32 = cpal::Sample::from_sample(sample);
                            buffer.push(sample_f32);

                            let abs_sample = sample_f32.abs();
                            max_amplitude = max_amplitude.max(abs_sample);

                            if abs_sample > threshold {
                                has_sound = true;
                            }
                        }
                    } else {
                        // Multi-channel: convert to mono by averaging channels
                        for chunk in data.chunks_exact(channels as usize) {
                            let mut sum = 0.0f32;
                            for &sample in chunk {
                                let sample_f32: f32 = cpal::Sample::from_sample(sample);
                                sum += sample_f32;
                            }
                            let mono_sample = sum / channels as f32;
                            buffer.push(mono_sample);

                            let abs_sample = mono_sample.abs();
                            max_amplitude = max_amplitude.max(abs_sample);

                            if abs_sample > threshold {
                                has_sound = true;
                            }
                        }
                    }

                    // Update current max amplitude (exponential moving average for smooth display)
                    {
                        let mut current_max = current_max_amplitude.lock().unwrap();
                        *current_max = current_max.max(max_amplitude) * 0.95; // Decay slowly
                    }

                    // Debug: Print max amplitude every 50 buffers (~1 second)
                    static mut BUFFER_COUNT: u32 = 0;
                    unsafe {
                        BUFFER_COUNT += 1;
                        if BUFFER_COUNT.is_multiple_of(50) {
                            println!(
                                "Max amplitude: {:.6}, Has sound: {}, Threshold: {}, Channels: {}",
                                max_amplitude, has_sound, threshold, channels
                            );
                        }
                    }

                    // Update last sound time if sound was detected
                    if has_sound {
                        let mut last_sound = last_sound_time.lock().unwrap();
                        *last_sound = Instant::now();
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| format!("Failed to build input stream: {}", e))?;

        Ok(stream)
    }

    pub fn save_audio_to_wav(
        &self,
        audio_data: &[f32],
        sample_rate: u32,
    ) -> Result<PathBuf, String> {
        // Trim leading silence but keep 0.2 seconds
        let keep_samples = (sample_rate as f32 * 0.2) as usize; // 0.2 seconds worth of samples

        let trimmed_data = trim_leading_silence(audio_data, self.silence_threshold, keep_samples);

        if trimmed_data.is_empty() {
            return Err("Audio data is empty after trimming".to_string());
        }

        // Create a temporary file
        let temp_file = tempfile::Builder::new()
            .prefix("winh_audio_")
            .suffix(".wav")
            .tempfile()
            .map_err(|e| format!("Failed to create temp file: {}", e))?;

        let temp_path = temp_file.path().to_path_buf();

        // Create WAV writer
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        println!(
            "Creating WAV file with: channels={}, sample_rate={}, bits_per_sample={}, samples={}",
            spec.channels,
            spec.sample_rate,
            spec.bits_per_sample,
            trimmed_data.len()
        );

        let mut writer = hound::WavWriter::create(&temp_path, spec)
            .map_err(|e| format!("Failed to create WAV writer: {}", e))?;

        // Write samples
        for &sample in trimmed_data {
            // Convert f32 to i16
            let sample_i16 = (sample * i16::MAX as f32) as i16;
            writer
                .write_sample(sample_i16)
                .map_err(|e| format!("Failed to write sample: {}", e))?;
        }

        writer
            .finalize()
            .map_err(|e| format!("Failed to finalize WAV file: {}", e))?;

        // Keep the file alive by forgetting the tempfile handle
        std::mem::forget(temp_file);

        println!(
            "Audio saved to: {:?} (trimmed {} samples from start)",
            temp_path,
            audio_data.len() - trimmed_data.len()
        );
        Ok(temp_path)
    }
}

impl Default for AudioRecorder {
    fn default() -> Self {
        Self::new(0.01).unwrap()
    }
}

fn trim_leading_silence(audio_data: &[f32], threshold: f32, keep_samples: usize) -> &[f32] {
    if audio_data.is_empty() {
        return audio_data;
    }

    // Find the first sample that exceeds the threshold
    let first_sound_idx = match audio_data
        .iter()
        .position(|&sample| sample.abs() > threshold)
    {
        Some(idx) => idx,
        None => return &[], // All samples are silent, return empty slice
    };

    // Find the last sample that exceeds the threshold
    let last_sound_idx = audio_data
        .iter()
        .rposition(|&sample| sample.abs() > threshold)
        .unwrap(); // Safe to unwrap because we know at least first_sound_idx exists

    // Calculate start index: go back by keep_samples, but not before 0
    let start_idx = first_sound_idx.saturating_sub(keep_samples);

    // Calculate end index: go forward by keep_samples, but not beyond the end
    let end_idx = (last_sound_idx + keep_samples + 1).min(audio_data.len());

    &audio_data[start_idx..end_idx]
}

/// Get list of available input devices
pub fn get_input_devices() -> Result<Vec<String>, String> {
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .map_err(|e| format!("Failed to get input devices: {}", e))?;

    let mut device_names = Vec::new();
    for device in devices {
        if let Ok(name) = device.name() {
            device_names.push(name);
        }
    }

    Ok(device_names)
}
