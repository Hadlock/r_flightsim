use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::settings::SharedVolume;

/// Resampled PCM samples ready for playback at the output device rate.
pub type PlaybackSamples = Vec<f32>;

/// Raw audio clip from synthesis (before resampling).
pub struct AudioClip {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

pub struct AudioPlayer {
    _stream: cpal::Stream,
    clip_queue: Arc<Mutex<VecDeque<PlaybackSamples>>>,
    output_sample_rate: u32,
}

impl AudioPlayer {
    pub fn new(vol_source: Option<SharedVolume>) -> Result<Self, Box<dyn std::error::Error>> {
        use cpal::traits::*;

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No audio output device")?;

        let supported_config = device.default_output_config()?;
        let output_sample_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels() as usize;

        let clip_queue: Arc<Mutex<VecDeque<PlaybackSamples>>> =
            Arc::new(Mutex::new(VecDeque::new()));
        let queue_clone = clip_queue.clone();

        let gap_samples = (output_sample_rate as f32 * 0.3) as u32; // 300ms gap

        let config = supported_config.config();

        // Audio callback state — all moved into the closure
        let mut current_clip: Option<PlaybackSamples> = None;
        let mut play_pos: usize = 0;
        let mut gap_remaining: u32 = 0;

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let volume = vol_source.as_ref().map_or(0.4, |v| v.get());
                let mut idx = 0;
                while idx < data.len() {
                    // Gap between clips
                    if gap_remaining > 0 {
                        for ch in 0..channels {
                            if idx + ch < data.len() {
                                data[idx + ch] = 0.0;
                            }
                        }
                        idx += channels;
                        gap_remaining -= 1;
                        continue;
                    }

                    // Playing a clip
                    if let Some(ref clip) = current_clip {
                        if play_pos < clip.len() {
                            let sample = clip[play_pos] * volume;
                            for ch in 0..channels {
                                if idx + ch < data.len() {
                                    data[idx + ch] = sample;
                                }
                            }
                            play_pos += 1;
                            idx += channels;
                            continue;
                        } else {
                            // Clip finished
                            current_clip = None;
                            play_pos = 0;
                            gap_remaining = gap_samples;
                            continue;
                        }
                    }

                    // Try next clip (non-blocking)
                    if let Ok(mut queue) = queue_clone.try_lock() {
                        if let Some(clip) = queue.pop_front() {
                            current_clip = Some(clip);
                            play_pos = 0;
                            continue;
                        }
                    }

                    // Silence
                    for ch in 0..channels {
                        if idx + ch < data.len() {
                            data[idx + ch] = 0.0;
                        }
                    }
                    idx += channels;
                }
            },
            move |err| {
                log::error!("Audio output error: {}", err);
            },
            None,
        )?;

        use cpal::traits::StreamTrait;
        stream.play()?;

        log::info!(
            "Audio output: {}Hz, {} channels",
            output_sample_rate,
            channels
        );

        Ok(AudioPlayer {
            _stream: stream,
            clip_queue,
            output_sample_rate,
        })
    }

    pub fn clip_queue(&self) -> Arc<Mutex<VecDeque<PlaybackSamples>>> {
        self.clip_queue.clone()
    }

    pub fn output_sample_rate(&self) -> u32 {
        self.output_sample_rate
    }
}

// ── Biquad filter ────────────────────────────────────────────────────

struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    /// RBJ Audio EQ Cookbook high-pass filter.
    fn high_pass(sample_rate: f32, cutoff: f32, q: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * cutoff / sample_rate;
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();

        let b0 = (1.0 + cos_w0) / 2.0;
        let b1 = -(1.0 + cos_w0);
        let b2 = (1.0 + cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Biquad {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// RBJ Audio EQ Cookbook low-pass filter.
    fn low_pass(sample_rate: f32, cutoff: f32, q: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * cutoff / sample_rate;
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();

        let b0 = (1.0 - cos_w0) / 2.0;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Biquad {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

// ── Radio filter ─────────────────────────────────────────────────────

/// Apply radio processing chain: bandpass → soft clip → noise → PTT clicks.
pub fn apply_radio_filter(samples: &mut Vec<f32>, sample_rate: u32) {
    let sr = sample_rate as f32;

    // 1. Bandpass: high-pass 300Hz + low-pass 3400Hz (radio bandwidth)
    let mut hp = Biquad::high_pass(sr, 300.0, 0.707);
    let mut lp = Biquad::low_pass(sr, 3400.0, 0.707);
    for sample in samples.iter_mut() {
        *sample = lp.process(hp.process(*sample));
    }

    // 2. Soft clip at 0.8 + normalize to 0.7
    for sample in samples.iter_mut() {
        if *sample > 0.8 {
            *sample = 0.8 + (*sample - 0.8).tanh() * 0.2;
        } else if *sample < -0.8 {
            *sample = -0.8 + (*sample + 0.8).tanh() * 0.2;
        }
    }
    let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if peak > 0.001 {
        let scale = 0.7 / peak;
        for sample in samples.iter_mut() {
            *sample *= scale;
        }
    }

    // 3. Subtle white noise bed (~-30dB ≈ 0.032)
    let noise_level = 0.032;
    let mut rng_state = 12345u32;
    for sample in samples.iter_mut() {
        rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
        let noise = ((rng_state >> 16) as f32 / 32768.0 - 1.0) * noise_level;
        *sample += noise;
    }

    // 4. PTT click transients (5ms) at start and end
    let click_len = (sr * 0.005) as usize;
    if samples.len() > click_len * 2 {
        for i in 0..click_len {
            let t = i as f32 / click_len as f32;
            let click =
                (t * std::f32::consts::PI * 2.0 * 1200.0 / sr).sin() * 0.3 * (1.0 - t);
            samples[i] += click;
        }
        let start = samples.len() - click_len;
        for i in 0..click_len {
            let t = i as f32 / click_len as f32;
            let click = (t * std::f32::consts::PI * 2.0 * 1200.0 / sr).sin() * 0.3 * t;
            samples[start + i] += click;
        }
    }
}

// ── Resampling ───────────────────────────────────────────────────────

/// Linear-interpolation resampler (sufficient for bandlimited radio audio).
pub fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }
    let ratio = to_rate as f64 / from_rate as f64;
    let new_len = (samples.len() as f64 * ratio) as usize;
    let mut output = Vec::with_capacity(new_len);
    for i in 0..new_len {
        let src_pos = i as f64 / ratio;
        let src_idx = src_pos as usize;
        let frac = (src_pos - src_idx as f64) as f32;
        let s0 = samples.get(src_idx).copied().unwrap_or(0.0);
        let s1 = samples.get(src_idx + 1).copied().unwrap_or(s0);
        output.push(s0 + (s1 - s0) * frac);
    }
    output
}
