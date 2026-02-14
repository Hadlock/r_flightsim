# CLAUDE.md — shaderflight: Piper TTS for ATC Radio

## Project Context

shaderflight is a wgpu flight simulator with Sobel wireframe rendering and an ATC radio
chatter system that generates authentic FAA-phraseology text transmissions. See previous
CLAUDE.md files for full architecture.

The ATC system (in `src/atc/`) generates `RadioMessage` structs with text, frequency,
speaker info, and a `voice_id: u8` field that was reserved for exactly this purpose.
Messages flow through `AtcManager` → `message_queue` → `message_log` and are displayed
as text in both an egui overlay and a ratatui terminal panel.

This task adds Piper TTS to speak those messages as audio.

---

## Voice Assets

Three Piper ONNX voices are in `assets/piper_voices/`:

```
assets/piper_voices/
├── voice_a.onnx
├── voice_a.onnx.json
├── voice_b.onnx
├── voice_b.onnx.json
├── voice_c.onnx
├── voice_c.onnx.json
```

Each `.onnx` is the model, the `.onnx.json` is the config (phoneme map, sample rate,
speaker IDs, etc.). The actual filenames may differ — use whatever is in the directory.
Load them at startup and keep them in memory.

---

## Architecture

```
src/tts/mod.rs       – TtsEngine: voice loading, synthesis, audio queue management
src/tts/audio.rs     – Audio output (cpal): mixing, playback, radio filter
```

Modify:
```
main.rs              – initialize TtsEngine, wire it to AtcManager output
src/atc/mod.rs       – when a message is delivered to message_log, also send it to TTS
Cargo.toml           – add dependencies
```

Do NOT modify: `renderer.rs`, shaders, `physics.rs`

---

## Dependencies

```toml
# ONNX Runtime for Piper inference
ort = "2"                          # onnxruntime Rust bindings
# Audio output
cpal = "0.15"                      # cross-platform audio
# Audio processing
rubato = "0.15"                    # sample rate conversion if needed
```

`ort` (the `ort` crate) is the maintained Rust ONNX Runtime binding. It bundles the
ONNX Runtime dylib or can link to a system install. Use the bundled/download strategy
so it just works on macOS M3:

```toml
ort = { version = "2", features = ["download-binaries"] }
```

---

## Voice Assignment

Each radio entity gets one of the three voices, assigned deterministically at startup:

```rust
pub struct VoiceAssignment {
    voice_index: usize,    // 0, 1, or 2 — index into loaded voices
    speed_factor: f32,     // speech rate multiplier (see below)
}
```

Assignment strategy:
- **Controllers** (NorCal Approach, SFO Tower, etc.): Pick from voices deterministically
  by hashing the facility name. Controllers should sound calm and measured.
  Speed factor: 1.1–1.2 (slightly faster than default, controllers are efficient).
- **Pilots** (AI planes 0–6): Assign by `plane_idx % 3`. Pilots can be slightly more
  varied in pace. Speed factor: 1.15–1.3 (pilots on busy frequencies talk briskly).
- **Ambient** (phantom callsigns): Alternate between voices. Speed factor: 1.2.

The `voice_id` field on `RadioMessage` maps to these assignments:
- `0–6`: pilot by plane index
- `100`: NorCal Approach controller
- `101`: SFO Tower controller
- `102–106`: other tower controllers
- `200–201`: ambient speakers

The TTS engine maintains a lookup table from `voice_id` → `VoiceAssignment`.

---

## Piper Inference Pipeline

Piper TTS works as follows:
1. **Text → Phoneme IDs**: Use the `.onnx.json` config to look up the phoneme map.
   Piper uses espeak-ng phonemes internally, but the model's JSON config contains
   a `phoneme_id_map` that maps characters to integer IDs. For English voices,
   you can feed raw text characters through this map (Piper's "text" input mode)
   or use espeak phonemes if available.
2. **Phoneme IDs → ONNX model**: The model takes `input` (int64 tensor of phoneme IDs)
   and `input_lengths` (int64 tensor, single value = length of input). It also takes
   `scales` (float32 tensor of 3 values: `[noise_scale, length_scale, noise_w]`).
   - `noise_scale`: controls expressiveness (0.667 is default)
   - `length_scale`: controls speed — **lower = faster**. Default is 1.0.
     For our "moderately fast radio" speech: use 0.75–0.85.
   - `noise_w`: controls phoneme duration variation (0.8 is default)
3. **Output**: The model produces a float32 audio tensor (raw PCM samples) at the
   sample rate specified in the JSON config (typically 22050 Hz).

### Speed Tuning

This is critical for radio realism. Real ATC radio is notably faster than conversational
speech but still intelligible. The `length_scale` parameter controls this directly:

```
length_scale = 1.0   → normal conversational speed (too slow for radio)
length_scale = 0.85  → moderately fast (good baseline for radio)
length_scale = 0.75  → brisk radio pace (controllers on busy frequency)
length_scale = 0.65  → too fast, starts to sound robotic
```

Map the `speed_factor` from voice assignment to `length_scale`:
```rust
let length_scale = 1.0 / speed_factor;  // speed_factor 1.2 → length_scale 0.833
```

---

## TTS Engine

```rust
pub struct TtsEngine {
    // Loaded ONNX sessions (one per voice file)
    voices: Vec<PiperVoice>,
    // voice_id → VoiceAssignment lookup
    assignments: HashMap<u8, VoiceAssignment>,
    // Synthesis queue: messages waiting to be synthesized
    synth_queue: Arc<Mutex<VecDeque<TtsRequest>>>,
    // Audio queue: synthesized PCM waiting to be played
    audio_queue: Arc<Mutex<VecDeque<AudioClip>>>,
    // Synthesis thread handle
    synth_thread: Option<JoinHandle<()>>,
    // Shutdown flag
    shutdown: Arc<AtomicBool>,
}

struct PiperVoice {
    session: ort::Session,
    config: PiperConfig,   // parsed from .onnx.json
    sample_rate: u32,      // from config, typically 22050
}

struct PiperConfig {
    phoneme_id_map: HashMap<char, Vec<i64>>,
    // Other config fields as needed
}

struct TtsRequest {
    text: String,
    voice_index: usize,
    speed_factor: f32,
}

struct AudioClip {
    samples: Vec<f32>,     // mono PCM, normalized -1.0 to 1.0
    sample_rate: u32,
}
```

### Threading Model

- **Synthesis thread**: A single dedicated thread that pulls from `synth_queue`,
  runs ONNX inference (this is the slow part, ~50-200ms per utterance on M3),
  and pushes results to `audio_queue`.
- **Audio thread**: Managed by cpal. Pulls from `audio_queue` and feeds the
  audio output device. Plays clips sequentially with a small gap (~300ms)
  between transmissions.
- **Main thread**: When `AtcManager` delivers a message to `message_log`,
  also push a `TtsRequest` to the synthesis queue.

This keeps synthesis off the main thread entirely. If synthesis can't keep up
(unlikely with 3 voices on M3 and one message every 5-15 seconds), messages
are dropped from the synth queue (oldest first).

---

## Radio Audio Effect (Post-Processing)

Raw Piper output sounds too clean for radio. Apply a simple processing chain
to each synthesized clip before queueing for playback:

1. **Bandpass filter**: Radio comms are bandlimited to roughly 300–3400 Hz.
   Apply a simple biquad bandpass (or high-pass at 300 Hz + low-pass at 3400 Hz).
   This is the single most important effect for radio realism.

2. **Light compression/clipping**: Radio audio is heavily compressed. Soft-clip
   the signal at ~0.8 amplitude, then normalize to 0.7. This flattens dynamics.

3. **Subtle noise bed**: Mix in very low-level white noise (-30dB) to simulate
   radio static. Just enough to notice subconsciously.

4. **Click transients**: Optionally add a very short (~5ms) click/pop at the
   start and end of each transmission to simulate the PTT (push-to-talk) keying.
   This is a surprisingly effective realism cue.

Implement these as simple sample-by-sample operations in `audio.rs`. No need for
an FFT or complex DSP library — biquad filters are ~10 lines of code.

### Biquad Bandpass (reference implementation)

```rust
struct Biquad {
    b0: f32, b1: f32, b2: f32,
    a1: f32, a2: f32,
    x1: f32, x2: f32,
    y1: f32, y2: f32,
}

impl Biquad {
    fn low_pass(sample_rate: f32, cutoff: f32, q: f32) -> Self { /* cookbook */ }
    fn high_pass(sample_rate: f32, cutoff: f32, q: f32) -> Self { /* cookbook */ }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
              - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}
```

Use Robert Bristow-Johnson's Audio EQ Cookbook formulas for the coefficients.

---

## Audio Playback (cpal)

```rust
pub struct AudioPlayer {
    stream: cpal::Stream,          // kept alive for lifetime of playback
    clip_queue: Arc<Mutex<VecDeque<AudioClip>>>,  // shared with TtsEngine
    current_clip: Option<AudioClip>,
    play_pos: usize,
    gap_remaining: u32,            // samples of silence between clips
}
```

### Setup

1. Open default output device via cpal
2. Get preferred output config (sample rate, channels)
3. If output sample rate ≠ voice sample rate (22050), use `rubato` to resample.
   Or: resample each clip after synthesis, before pushing to audio_queue, so the
   audio callback is simple.
4. Build the cpal output stream with a callback that:
   - If playing a clip: copy samples to output buffer, advance position
   - If clip finished: insert gap_remaining samples of silence (300ms worth),
     then pop next clip from queue
   - If queue empty: output silence
5. Handle mono→stereo: duplicate mono samples to both channels if output is stereo

### Volume

ATC radio shouldn't be deafening. Default to ~40% of max volume. Eventually make
this configurable in Settings, but hardcode for now.

---

## Integration with AtcManager

In `atc/mod.rs`, when messages move from `message_queue` to `message_log`:

```rust
// In the tick() method, where messages are delivered:
while let Some(msg) = self.message_queue.front() {
    if msg.timestamp <= self.sim_time {
        let msg = self.message_queue.pop_front().unwrap();
        // NEW: send to TTS
        if let Some(tts_tx) = &self.tts_sender {
            let _ = tts_tx.send(TtsRequest {
                text: msg.text.clone(),
                voice_index: /* lookup from voice_id */,
                speed_factor: /* lookup from voice_id */,
            });
        }
        self.message_log.push_back(msg);
        // ... trim log
    } else {
        break;
    }
}
```

Use a `crossbeam_channel` or `std::sync::mpsc` sender stored on `AtcManager` to
decouple message delivery from synthesis. The TTS thread owns the receiver.

### Text Cleanup for TTS

Before sending text to Piper, clean up display-oriented formatting:
- Strip callsign formatting artifacts
- Ensure hyphens in spoken numbers are preserved (Piper handles "niner-seven-bravo" fine)
- The FAA phraseology text is already designed to be spoken, so minimal cleanup needed

---

## Phoneme Mapping (Piper Text Mode)

Piper's `.onnx.json` contains a `phoneme_id_map` that maps characters to phoneme IDs.
For English models in "text" mode (not espeak phoneme mode), the mapping is typically:

```json
{
  "phoneme_id_map": {
    " ": [3],
    "a": [10],
    "b": [11],
    ...
  }
}
```

The input pipeline:
1. Lowercase the text
2. Strip characters not in the phoneme map
3. Add beginning-of-sequence and end-of-sequence tokens if the config specifies them
   (check `phoneme_type` in config — if "espeak", you need espeak; if "text", direct char mapping)
4. Insert padding tokens between phoneme IDs if the config's `phoneme_id_map` indicates
   an `_` or `^`/`$` for BOS/EOS (model-specific, check the JSON)
5. Build the int64 tensor and run inference

**Important**: Many Piper voices use espeak phonemes, not raw text. Check the
`phoneme_type` field in the JSON config. If it says `"espeak"`, you'll need to either:
- Shell out to `espeak-ng --ipa` to phonemize text first (simplest, espeak-ng is ~5ms)
- Use the `espeak-ng` C library via FFI
- Use the `piper-rs` crate if one exists that handles this

If the voices are English text-mode voices, direct character mapping works and is simpler.

---

## Startup Sequence

1. Scan `assets/piper_voices/` for `.onnx` files
2. For each, load the ONNX session via `ort` and parse the companion `.onnx.json`
3. Build voice assignment table (voice_id → voice_index + speed_factor)
4. Spawn synthesis thread
5. Initialize cpal audio output
6. Pass the TTS sender channel to AtcManager

If voice loading fails (missing files, ONNX error), log a warning and continue
without TTS. The text display system works independently.

---

## Performance Budget

On M3 MacBook Pro:
- Piper ONNX inference for a typical ATC sentence (~5-15 words): **50–200ms**
- Messages arrive every 5–15 seconds
- Synthesis thread has plenty of headroom (one synthesis per ~5s, each taking <200ms)
- Audio output is trivial CPU cost
- The radio filter (biquad + clipping) is negligible

If synthesis occasionally takes longer (complex sentence, system load), the audio
gap between transmissions just grows slightly — which actually sounds natural on radio.

---

## CLI Integration

Add a flag to disable TTS (useful for development, testing, or systems without audio):

```rust
// In cli.rs Args struct:
/// Disable TTS audio for ATC radio
#[arg(long = "no-tts")]
pub no_tts: bool,
```

When `--no-tts` is set, skip TTS initialization entirely. Text display still works.

---

## Do NOT

- Modify `renderer.rs` or shaders
- Block the main thread on ONNX inference
- Use a TTS crate that bundles its own massive runtime (keep it lean with ort + piper models)
- Implement voice activity detection or echo cancellation (not needed)
- Try to lip-sync or animate anything to the audio
- Play audio during Menu state (only during Flying state)
- Panic if audio device is unavailable — gracefully fall back to text-only

---

## Verification

After implementation, `cargo run --release -- -i` should:
- Play synthesized speech through the speakers for each ATC radio transmission
- Audio should sound like radio (bandpassed, slightly compressed, with PTT clicks)
- Different speakers should have distinguishable voices
- Speech pace should be moderately fast — brisk but intelligible (like real ATC)
- No audio glitches, pops, or underruns during normal flight
- Text overlay and terminal log continue to work alongside audio
- `cargo run --release -- -i --no-tts` should work silently with text only