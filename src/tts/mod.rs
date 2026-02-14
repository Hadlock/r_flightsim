pub mod audio;

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;

use audio::{AudioClip, PlaybackSamples};

// ── Public types ─────────────────────────────────────────────────────

/// A request to synthesize speech.
pub struct TtsRequest {
    pub text: String,
    pub voice_index: usize,
    pub speed_factor: f32,
}

/// Voice assignment for a speaker (voice_id → voice + speed).
struct VoiceAssignment {
    voice_index: usize,
    speed_factor: f32,
}

/// Clonable handle for sending TTS requests. Stored in AtcManager.
pub struct TtsSender {
    sender: mpsc::Sender<TtsRequest>,
    assignments: Arc<HashMap<u8, VoiceAssignment>>,
}

impl TtsSender {
    /// Queue a message for synthesis. voice_id selects the voice + speed.
    pub fn send(&self, voice_id: u8, text: &str) {
        if let Some(a) = self.assignments.get(&voice_id) {
            let _ = self.sender.send(TtsRequest {
                text: text.to_string(),
                voice_index: a.voice_index,
                speed_factor: a.speed_factor,
            });
        }
    }
}

impl Clone for TtsSender {
    fn clone(&self) -> Self {
        TtsSender {
            sender: self.sender.clone(),
            assignments: self.assignments.clone(),
        }
    }
}

// ── Piper voice ──────────────────────────────────────────────────────

struct PiperConfig {
    phoneme_id_map: HashMap<String, Vec<i64>>,
    sample_rate: u32,
    noise_scale: f32,
    noise_w: f32,
}

struct PiperVoice {
    session: ort::session::Session,
    config: PiperConfig,
}

// ── TTS Engine ───────────────────────────────────────────────────────

pub struct TtsEngine {
    _synth_thread: Option<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
    sender: mpsc::Sender<TtsRequest>,
    assignments: Arc<HashMap<u8, VoiceAssignment>>,
    #[allow(dead_code)]
    audio_player: audio::AudioPlayer,
}

impl TtsEngine {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Check espeak-ng availability
        let has_espeak = Command::new("espeak-ng")
            .arg("--version")
            .output()
            .is_ok();
        if !has_espeak {
            log::warn!("espeak-ng not found. Install with: brew install espeak-ng");
            log::warn!("TTS will be disabled.");
            return Err("espeak-ng not found".into());
        }

        // Scan voice directory
        let voice_dir = Path::new("assets/piper_voice");
        let mut voices = Vec::new();

        if voice_dir.exists() {
            let mut entries: Vec<_> = std::fs::read_dir(voice_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map_or(false, |ext| ext == "onnx")
                })
                .collect();
            entries.sort_by_key(|e| e.path());

            for entry in entries {
                let onnx_path = entry.path();
                let json_path = format!("{}.json", onnx_path.display());
                let json_path = Path::new(&json_path);

                if !json_path.exists() {
                    log::warn!("Missing config for {:?}, skipping", onnx_path);
                    continue;
                }
                log::info!("Loading voice: {:?}", onnx_path);

                match load_voice(&onnx_path, json_path) {
                    Ok(voice) => voices.push(voice),
                    Err(e) => log::warn!("Failed to load {:?}: {}", onnx_path, e),
                }
            }
        }

        if voices.is_empty() {
            return Err("No Piper voices found in assets/piper_voice/".into());
        }
        log::info!("Loaded {} Piper voices", voices.len());

        // Build assignments
        let assignments = Arc::new(build_assignments(voices.len()));

        // Audio player
        let audio_player = audio::AudioPlayer::new()?;
        let clip_queue = audio_player.clip_queue();
        let output_sr = audio_player.output_sample_rate();

        // Channel
        let (sender, receiver) = mpsc::channel::<TtsRequest>();
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        // Synthesis thread
        let synth_thread = std::thread::Builder::new()
            .name("tts-synth".to_string())
            .spawn(move || {
                synth_loop(receiver, voices, shutdown_clone, clip_queue, output_sr);
            })?;

        Ok(TtsEngine {
            _synth_thread: Some(synth_thread),
            shutdown,
            sender,
            assignments,
            audio_player,
        })
    }

    /// Create a TtsSender handle for the AtcManager.
    pub fn tts_sender(&self) -> TtsSender {
        TtsSender {
            sender: self.sender.clone(),
            assignments: self.assignments.clone(),
        }
    }
}

impl Drop for TtsEngine {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self._synth_thread.take() {
            let _ = handle.join();
        }
    }
}

// ── Voice loading ────────────────────────────────────────────────────

fn load_voice(
    onnx_path: &Path,
    json_path: &Path,
) -> Result<PiperVoice, Box<dyn std::error::Error>> {
    let json_str = std::fs::read_to_string(json_path)?;
    let json: serde_json::Value = serde_json::from_str(&json_str)?;

    let sample_rate = json["audio"]["sample_rate"]
        .as_u64()
        .unwrap_or(22050) as u32;
    let noise_scale = json["inference"]["noise_scale"]
        .as_f64()
        .unwrap_or(0.667) as f32;
    let noise_w = json["inference"]["noise_w"]
        .as_f64()
        .unwrap_or(0.8) as f32;

    let mut phoneme_id_map = HashMap::new();
    if let Some(map) = json["phoneme_id_map"].as_object() {
        for (key, val) in map {
            if let Some(arr) = val.as_array() {
                let ids: Vec<i64> = arr.iter().filter_map(|v| v.as_i64()).collect();
                phoneme_id_map.insert(key.clone(), ids);
            }
        }
    }

    let config = PiperConfig {
        phoneme_id_map,
        sample_rate,
        noise_scale,
        noise_w,
    };

    let session = ort::session::Session::builder()?.commit_from_file(onnx_path)?;

    Ok(PiperVoice { session, config })
}

// ── Voice assignment ─────────────────────────────────────────────────

fn build_assignments(num_voices: usize) -> HashMap<u8, VoiceAssignment> {
    let mut m = HashMap::new();

    // Pilots 0–6: voice by plane_idx % num_voices, speed 1.29–1.46
    for i in 0..7u8 {
        m.insert(
            i,
            VoiceAssignment {
                voice_index: (i as usize) % num_voices,
                speed_factor: 1.29 + (i as f32 * 0.028),
            },
        );
    }

    // Controllers: deterministic by facility
    // 100 = NorCal Approach
    m.insert(
        100,
        VoiceAssignment {
            voice_index: 0 % num_voices,
            speed_factor: 1.29,
        },
    );
    // 101 = SFO Tower
    m.insert(
        101,
        VoiceAssignment {
            voice_index: 1 % num_voices,
            speed_factor: 1.23,
        },
    );
    // 102–106 other controllers
    for i in 102..=106u8 {
        m.insert(
            i,
            VoiceAssignment {
                voice_index: ((i - 100) as usize) % num_voices,
                speed_factor: 1.23 + ((i - 102) as f32 * 0.022),
            },
        );
    }

    // Ambient 200–201
    m.insert(
        200,
        VoiceAssignment {
            voice_index: 0 % num_voices,
            speed_factor: 1.34,
        },
    );
    m.insert(
        201,
        VoiceAssignment {
            voice_index: 2.min(num_voices - 1),
            speed_factor: 1.34,
        },
    );

    m
}

// ── Synthesis thread ─────────────────────────────────────────────────

fn synth_loop(
    receiver: mpsc::Receiver<TtsRequest>,
    mut voices: Vec<PiperVoice>,
    shutdown: Arc<AtomicBool>,
    clip_queue: Arc<Mutex<VecDeque<PlaybackSamples>>>,
    output_sample_rate: u32,
) {
    while !shutdown.load(Ordering::Relaxed) {
        match receiver.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(request) => {
                let voice_idx = request.voice_index.min(voices.len() - 1);
                let voice = &mut voices[voice_idx];

                match synthesize(voice, &request.text, request.speed_factor) {
                    Ok(mut clip) => {
                        // Apply radio filter at native sample rate
                        audio::apply_radio_filter(&mut clip.samples, clip.sample_rate);

                        // Resample to output device rate
                        let resampled = audio::resample_linear(
                            &clip.samples,
                            clip.sample_rate,
                            output_sample_rate,
                        );

                        let mut queue = clip_queue.lock().unwrap();
                        // Drop oldest if queue is backed up
                        while queue.len() > 5 {
                            queue.pop_front();
                        }
                        queue.push_back(resampled);
                    }
                    Err(e) => {
                        log::warn!("TTS synthesis failed: {}", e);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

// ── ONNX inference ───────────────────────────────────────────────────

fn synthesize(
    voice: &mut PiperVoice,
    text: &str,
    speed_factor: f32,
) -> Result<AudioClip, Box<dyn std::error::Error>> {
    // Phonemize with espeak-ng
    let phonemes = phonemize(text)?;

    // Map phonemes to IDs
    let phoneme_ids = phonemes_to_ids(&phonemes, &voice.config.phoneme_id_map);
    if phoneme_ids.is_empty() {
        return Err("Empty phoneme sequence".into());
    }

    let length_scale = 1.0 / speed_factor;
    let seq_len = phoneme_ids.len();

    // Build input tensors using ort's Tensor::from_array((shape, data))
    let input = ort::value::Tensor::<i64>::from_array((
        [1, seq_len],
        phoneme_ids.into_boxed_slice(),
    ))?;
    let input_lengths = ort::value::Tensor::<i64>::from_array((
        [1usize],
        vec![seq_len as i64].into_boxed_slice(),
    ))?;
    let scales = ort::value::Tensor::<f32>::from_array((
        [3usize],
        vec![voice.config.noise_scale, length_scale, voice.config.noise_w].into_boxed_slice(),
    ))?;

    // Run inference
    let outputs = voice.session.run(ort::inputs![
        "input" => input,
        "input_lengths" => input_lengths,
        "scales" => scales,
    ])?;

    // Extract audio samples from output tensor
    let (_, audio_data) = outputs[0].try_extract_tensor::<f32>()?;
    let samples: Vec<f32> = audio_data.to_vec();

    Ok(AudioClip {
        samples,
        sample_rate: voice.config.sample_rate,
    })
}

// ── Phonemization ────────────────────────────────────────────────────

/// Run espeak-ng to convert text to IPA phonemes.
fn phonemize(text: &str) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("espeak-ng")
        .args(["--ipa", "-q", text])
        .output()?;

    if !output.status.success() {
        return Err(format!(
            "espeak-ng failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    let phonemes = String::from_utf8(output.stdout)?;
    Ok(phonemes.trim().to_string())
}

/// Convert IPA phoneme string to Piper phoneme ID sequence.
/// Inserts pad tokens between phonemes and wraps with BOS/EOS.
fn phonemes_to_ids(phonemes: &str, map: &HashMap<String, Vec<i64>>) -> Vec<i64> {
    let mut ids = Vec::new();

    // BOS token (^)
    if let Some(bos) = map.get("^") {
        ids.extend(bos);
    }
    // Pad after BOS
    if let Some(pad) = map.get("_") {
        ids.extend(pad);
    }

    for ch in phonemes.chars() {
        let key = ch.to_string();
        if let Some(phoneme_ids) = map.get(&key) {
            ids.extend(phoneme_ids);
            // Pad between phonemes
            if let Some(pad) = map.get("_") {
                ids.extend(pad);
            }
        }
        // Skip unmapped characters silently
    }

    // EOS token ($)
    if let Some(eos) = map.get("$") {
        ids.extend(eos);
    }

    ids
}
