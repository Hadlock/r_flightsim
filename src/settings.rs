use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Lock-free f32 volume readable from audio threads.
#[derive(Clone)]
pub struct SharedVolume(Arc<AtomicU32>);

impl SharedVolume {
    pub fn new(initial: f32) -> Self {
        Self(Arc::new(AtomicU32::new(initial.to_bits())))
    }

    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    pub fn set(&self, value: f32) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }
}

pub struct Settings {
    pub music_volume: SharedVolume,
    pub atc_voice_volume: SharedVolume,
    pub engine_volume: SharedVolume,
    pub fetch_orbital_params: bool,
}

impl Settings {
    pub fn new() -> Self {
        Self {
            music_volume: SharedVolume::new(0.15),
            atc_voice_volume: SharedVolume::new(0.38),
            engine_volume: SharedVolume::new(0.35),
            fetch_orbital_params: false,
        }
    }
}
