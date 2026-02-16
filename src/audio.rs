use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use rand::seq::SliceRandom;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use serde::Deserialize;

use crate::settings::SharedVolume;

// ── Music Player ─────────────────────────────────────────────────────

pub struct MusicPlayer {
    _stream: OutputStream,
    _handle: OutputStreamHandle,
    sink: Sink,
    playlist: Vec<PathBuf>,
    current_index: usize,
    last_played: Option<usize>,
    volume: SharedVolume,
}

impl MusicPlayer {
    pub fn new(music_dir: &Path, volume: SharedVolume) -> Option<Self> {
        let mut files: Vec<PathBuf> = fs::read_dir(music_dir)
            .ok()?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .map_or(false, |ext| ext.eq_ignore_ascii_case("flac"))
            })
            .collect();

        if files.is_empty() {
            log::warn!("No FLAC files found in {}", music_dir.display());
            return None;
        }

        let mut rng = rand::thread_rng();
        files.shuffle(&mut rng);

        let (stream, handle) = OutputStream::try_default().ok()?;
        let sink = Sink::try_new(&handle).ok()?;
        sink.set_volume(volume.get());

        let mut player = MusicPlayer {
            _stream: stream,
            _handle: handle,
            sink,
            playlist: files,
            current_index: 0,
            last_played: None,
            volume,
        };

        player.enqueue_current();
        Some(player)
    }

    fn enqueue_current(&mut self) {
        if self.playlist.is_empty() {
            return;
        }

        // Skip if next would repeat last played
        if let Some(last) = self.last_played {
            if self.current_index == last && self.playlist.len() > 1 {
                self.current_index = (self.current_index + 1) % self.playlist.len();
            }
        }

        let path = &self.playlist[self.current_index];
        match fs::File::open(path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                match Decoder::new(reader) {
                    Ok(source) => {
                        self.sink.append(source);
                        self.last_played = Some(self.current_index);
                        log::info!("Playing music: {}", path.display());
                    }
                    Err(e) => {
                        log::warn!("Failed to decode {}: {}", path.display(), e);
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to open {}: {}", path.display(), e);
            }
        }
    }

    pub fn tick(&mut self) {
        self.sink.set_volume(self.volume.get());

        if self.sink.empty() {
            self.current_index += 1;

            // Reshuffle when playlist exhausted
            if self.current_index >= self.playlist.len() {
                self.current_index = 0;
                let mut rng = rand::thread_rng();
                self.playlist.shuffle(&mut rng);
            }

            self.enqueue_current();
        }
    }
}

// ── Engine Sound ─────────────────────────────────────────────────────

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum EngineSoundCategory {
    JetLarge,
    JetSmall,
    PropellerLarge,
    PropellerSmall,
    Space,
}

impl EngineSoundCategory {
    pub fn default_wav_path(&self) -> PathBuf {
        let name = match self {
            Self::JetLarge => "default-jet-large",
            Self::JetSmall => "default-jet-small",
            Self::PropellerLarge => "default-propeller-large",
            Self::PropellerSmall => "default-propeller-small",
            Self::Space => "default-space",
        };
        PathBuf::from(format!("assets/engine_noise/{}.wav", name))
    }
}

pub struct EngineSoundPlayer {
    _stream: OutputStream,
    _handle: OutputStreamHandle,
    sink: Sink,
    volume: SharedVolume,
}

impl EngineSoundPlayer {
    pub fn new(category: &EngineSoundCategory, volume: SharedVolume) -> Option<Self> {
        let path = category.default_wav_path();
        if !path.exists() {
            log::warn!("Engine sound not found: {}", path.display());
            return None;
        }

        let (stream, handle) = OutputStream::try_default().ok()?;
        let sink = Sink::try_new(&handle).ok()?;
        sink.set_volume(volume.get());

        let file = fs::File::open(&path).ok()?;
        let reader = BufReader::new(file);
        match Decoder::new(reader) {
            Ok(source) => {
                sink.append(source.repeat_infinite());
                log::info!("Engine sound looping: {}", path.display());
            }
            Err(e) => {
                log::warn!("Failed to decode engine sound {}: {}", path.display(), e);
                return None;
            }
        }

        Some(EngineSoundPlayer {
            _stream: stream,
            _handle: handle,
            sink,
            volume,
        })
    }

    /// Update volume and pitch. `throttle` is 0.0–1.0.
    pub fn tick(&self, throttle: f32) {
        self.sink.set_volume(self.volume.get());
        let speed = 0.35 + throttle * 1.1;
        self.sink.set_speed(speed);
    }
}
