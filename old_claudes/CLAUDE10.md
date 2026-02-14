# CLAUDE.md — shaderflight: ATC Radio Chatter System

## Project Context

shaderflight is a wgpu flight simulator with Sobel edge-detection wireframe rendering.
All world math is WGS-84 ECEF with ENU local frames. See main CLAUDE.md for architecture.

Relevant existing systems:
```
ai_traffic.rs    – 5 AI Ki-61 planes flying figure-8s between 3 Bay Area waypoints
telemetry.rs     – ratatui terminal dashboard (shared telemetry via Arc<Mutex<T>>)
menu.rs          – egui overlay system (already integrated with wgpu)
coords.rs        – lla_to_ecef, ecef_to_lla, enu_frame_at
sim.rs           – SimRunner, player aircraft state
```

The AI traffic system has 5 planes with positions, headings, speeds, altitudes, and a
NavState (Loiter/Transit). This ATC system hooks into that state to generate contextual
radio transmissions.

---

## Task: ATC Radio Chatter Generator

Generate authentic FAA-phraseology radio transmissions for AI traffic operating around
SF Bay Area airports. The player eavesdrops on radio traffic (no player participation yet).
Display as both an on-screen text overlay (egui) and a terminal radio log (ratatui).

### Files to create

```
src/atc/mod.rs           – AtcManager, tick logic, message routing
src/atc/phraseology.rs   – message templates, callsign generation, FAA formatting
src/atc/facilities.rs    – airport/facility definitions, frequencies, runways
src/atc/types.rs         – shared types (RadioMessage, Frequency, Callsign, etc.)
```

Modify:
```
main.rs          – instantiate AtcManager, tick it, pass messages to display
ai_traffic.rs    – add ATC state per plane (callsign, current frequency, flight phase)
telemetry.rs     – add radio log panel to the ratatui dashboard
menu.rs          – add small radio text overlay during Flying state (or a new egui layer)
```

Do NOT modify: `renderer.rs`, shaders, `physics.rs`

---

## ATC Facility Model

### Hierarchy (simplified for Bay Area)

```
NorCal Approach (TRACON)
├── SFO Tower          ← Class B, the big one
├── OAK Tower          ← Class C
├── SJC Tower          ← Class C
├── HWD Tower          ← Hayward, Class D
├── PAO Tower          ← Palo Alto, Class D
└── SQL Tower          ← San Carlos, Class D
```

No Ground controllers. No Oakland Center. No ATIS. Keep it tight.

### Facility Definition

```rust
pub struct AtcFacility {
    name: &'static str,            // "SFO Tower", "NorCal Approach"
    callsign: &'static str,        // "San Francisco Tower", "NorCal Approach"
    facility_type: FacilityType,   // Tower or Approach
    frequency: f32,                // MHz, e.g. 120.5
    airport_ident: Option<&'static str>,  // "KSFO", "KOAK", etc.
    position: (f64, f64),          // lat, lon (for distance calculations)
    // Tower-specific
    active_runways: Vec<Runway>,
}

pub enum FacilityType {
    Approach,   // NorCal — handles transit, handoffs
    Tower,      // Per-airport — handles takeoff, landing, pattern
}

pub struct Runway {
    pub designator: &'static str,  // "28L", "19R"
    pub heading: f64,              // magnetic heading in degrees
}
```

### Real frequencies (use these for authenticity)

```rust
// NorCal Approach sectors (simplified — use one primary freq)
NORCAL_APPROACH: 135.65

// Tower frequencies
KSFO_TOWER: 120.5
KOAK_TOWER: 118.3
KSJC_TOWER: 124.0
KHWD_TOWER: 120.2
KPAO_TOWER: 118.6
KSQL_TOWER: 119.2
```

Active runways (pick one configuration, don't rotate):
- KSFO: 28L, 28R (westbound ops, most common)
- KOAK: 30
- KSJC: 30L
- KHWD: 28L
- KPAO: 31
- KSQL: 30

---

## AI Plane ATC State

Each AI plane in `ai_traffic.rs` gets additional ATC state:

```rust
pub struct AiPlaneAtcState {
    pub callsign: Callsign,           // e.g. "Ki-61 niner-seven-bravo"
    pub squawk: u16,                   // 4-digit transponder code
    pub current_freq: f32,             // what frequency they're tuned to
    pub flight_phase: FlightPhase,     // drives what ATC messages are generated
    pub last_transmission: f64,        // timestamp of last radio call (prevent spam)
}

pub enum FlightPhase {
    // Loitering planes (most of the 5 AI planes)
    EnRoute,              // flying between waypoints, on NorCal Approach

    // Pattern plane (1 dedicated plane doing touch-and-go at SFO)
    Downwind,             // parallel to runway, opposite direction
    Base,                 // turning toward runway
    Final,                // lined up, descending
    TouchAndGo,           // on the runway, about to go around
    Crosswind,            // climbing out, turning crosswind
    Departure,            // climbing, about to re-enter downwind
}
```

### Callsign Format

Use the real convention: aircraft type + tail number.

```rust
pub struct Callsign {
    pub aircraft_type: &'static str,   // "Ki-61" — shortened type name
    pub tail: String,                   // "97B", "42A", "66C" etc.
}
```

Generate callsigns like: "Ki-61 niner-seven-bravo", "Ki-61 four-two-alpha".

When a controller shortens it (after initial contact): "niner-seven-bravo".

Phonetic alphabet for tail letters: Alpha, Bravo, Charlie, Delta, Echo (one per plane).
Tail numbers: random 2-digit, seeded. E.g. "97B", "42A", "66C", "31D", "58E".

---

## Message Generation

### RadioMessage

```rust
pub struct RadioMessage {
    pub timestamp: f64,        // sim time
    pub frequency: f32,        // MHz
    pub speaker: Speaker,      // who's talking
    pub text: String,          // the spoken text
    pub text_display: String,  // formatted for display (may include frequency tag)
}

pub enum Speaker {
    Pilot(usize),              // AI plane index
    Controller(String),        // facility name
}
```

### Transmission Pairs

Most ATC communications are call-and-response pairs. The manager generates both with a
realistic delay between them:

```
[T+0.0s]  Pilot: "NorCal Approach, Ki-61 niner-seven-bravo, level two thousand four hundred"
[T+2.5s]  ATC:   "Ki-61 niner-seven-bravo, NorCal Approach, radar contact, squawk two-four-five-one"
```

Delay between pilot call and controller response: 1.5–4.0 seconds (randomized).
Delay between transmission pairs from the same plane: minimum 20 seconds.
Overall system: aim for roughly one transmission every 5–15 seconds across all planes
and facilities. Enough to sound like a moderately busy frequency, not overwhelming.

### Message Templates by Flight Phase

#### EnRoute (planes doing figure-8s, on NorCal Approach)

Initial contact (once, when transitioning to transit between waypoints):
```
Pilot: "NorCal Approach, Ki-61 {callsign}, {altitude} feet, proceeding direct {nearest_waypoint_name}"
ATC:   "Ki-61 {callsign}, NorCal Approach, radar contact, squawk {squawk}"
```

Periodic check-in (every 60–120 seconds while en route):
```
Pilot: "NorCal Approach, Ki-61 {callsign}, level {altitude}"
ATC:   "Ki-61 {callsign}, roger"
```

Handoff when approaching an airport's airspace (within 8km of a towered field):
```
ATC:   "Ki-61 {callsign}, contact {airport} Tower on {frequency}"
Pilot: "{airport} Tower, Ki-61 {callsign}, {altitude} feet, {distance} miles {direction}"
Tower: "Ki-61 {callsign}, {airport} Tower, altimeter {altimeter}, report {landmark_or_position}"
```

#### Touch-and-Go Pattern at SFO (1 dedicated plane)

Downwind:
```
Pilot: "San Francisco Tower, Ki-61 {callsign}, left downwind runway two-eight left"
Tower: "Ki-61 {callsign}, number {sequence}, follow traffic on base"
```

or:
```
Tower: "Ki-61 {callsign}, San Francisco Tower, cleared for the option runway two-eight left"
Pilot: "Cleared for the option two-eight left, {callsign}"
```

Base turn:
```
Pilot: "Ki-61 {callsign}, turning base"
Tower: "Ki-61 {callsign}, roger"
```

Final:
```
Pilot: "Ki-61 {callsign}, short final two-eight left"
Tower: "Ki-61 {callsign}, cleared touch and go runway two-eight left, wind two-seven-zero at {wind_speed}"
Pilot: "Cleared touch and go, {callsign}"
```

After touch-and-go, departing:
```
Tower: "Ki-61 {callsign}, make left crosswind departure, contact NorCal on one-three-five-point-six-five when able"
Pilot: "Left crosswind, NorCal on thirty-five sixty-five, {callsign}"
```

Re-entering downwind (back on tower freq):
```
Pilot: "San Francisco Tower, Ki-61 {callsign}, re-entering left downwind two-eight left, one thousand five hundred"
Tower: "Ki-61 {callsign}, report midfield downwind"
```

#### Occasional Traffic Advisories (NorCal Approach, adds realism)

```
ATC: "Ki-61 {callsign}, traffic twelve o'clock, {distance} miles, {altitude}, type unknown"
Pilot: "Looking for traffic, {callsign}"
```

or:
```
Pilot: "NorCal, Ki-61 {callsign}, requesting flight following"
ATC: "Ki-61 {callsign}, squawk {squawk}, say altitude"
Pilot: "{altitude} feet, {callsign}"
ATC: "Ki-61 {callsign}, radar contact, {position}, altimeter three-zero-one-niner"
```

---

## FAA Phraseology Rules

These matter for authenticity. Implement them in `phraseology.rs`:

### Numbers
- Altitudes: spoken in hundreds/thousands. "two thousand four hundred", "one thousand five hundred"
- Headings: three digits, spoken individually. "heading two-seven-zero"
- Runways: spoken individually. "runway two-eight left"
- Frequencies: spoken with "point". "one-two-zero-point-five" or "thirty-five sixty-five" (informal shorthand for 135.65)
- Squawk codes: four digits spoken individually. "squawk two-four-five-one"
- Altimeter settings: "altimeter three-zero-one-niner"

### Digit pronunciation
```
0 = "zero"
1 = "one"
2 = "two"
3 = "three" (some say "tree" but use "three")
4 = "four"
5 = "five" (some say "fife" but use "five")
6 = "six"
7 = "seven"
8 = "eight"
9 = "niner"  ← always "niner", never "nine"
```

### Phonetic alphabet (for tail letters)
```
A = Alpha, B = Bravo, C = Charlie, D = Delta, E = Echo
```

### Key phrases
- "Roger" = understood
- "Wilco" = will comply
- "Cleared for the option" = cleared for touch-and-go, stop-and-go, or full stop (pilot's choice)
- "Say again" = repeat
- "Affirmative" = yes
- "Negative" = no
- Readback: pilot reads back clearances. Controller reads back only critical items.
- Callsign at END of pilot transmissions: "Cleared to land two-eight left, niner-seven-bravo"
- Callsign at BEGINNING of controller transmissions: "Ki-61 niner-seven-bravo, cleared to land..."

### Wind
- "Wind two-seven-zero at eight" = 270° at 8 knots
- Use fixed wind for now: 270° at 8 knots (typical SFO westerly)

### Altimeter
- Fixed for now: "three-zero-one-niner" (30.19 inHg, reasonable Bay Area value)

---

## AtcManager

```rust
pub struct AtcManager {
    facilities: Vec<AtcFacility>,
    message_queue: VecDeque<RadioMessage>,    // upcoming messages (scheduled)
    message_log: VecDeque<RadioMessage>,      // recent messages (for display)
    max_log_size: usize,                       // keep last ~50 messages
    sim_time: f64,
    rng: StdRng,
}
```

### Tick Logic (~1Hz is fine, no need to run every frame)

```
fn tick(&mut self, dt: f64, ai_planes: &[AiPlane], player_pos: DVec3) {
    self.sim_time += dt;

    // 1. Drain any scheduled messages whose timestamp has arrived
    while let Some(msg) = self.message_queue.front() {
        if msg.timestamp <= self.sim_time {
            let msg = self.message_queue.pop_front().unwrap();
            self.message_log.push_back(msg);
            // trim log
        } else {
            break;
        }
    }

    // 2. For each AI plane, check if its flight phase triggers a new transmission
    for (i, plane) in ai_planes.iter().enumerate() {
        if self.should_generate_transmission(i, plane) {
            let messages = self.generate_transmission(i, plane);
            for msg in messages {
                self.message_queue.push_back(msg);
            }
        }
    }

    // 3. Occasionally generate ambient transmissions (traffic advisories, etc.)
    //    to fill quiet gaps
}
```

### Message Availability for Display

```rust
/// Get recent messages for display. Returns messages from the last N seconds.
pub fn recent_messages(&self, seconds: f64) -> Vec<&RadioMessage> {
    self.message_log.iter()
        .filter(|m| self.sim_time - m.timestamp < seconds)
        .collect()
}

/// Get the most recent message (for the on-screen overlay)
pub fn latest_message(&self) -> Option<&RadioMessage> {
    self.message_log.back()
}
```

---

## Display: On-Screen Radio Overlay (egui)

Small semi-transparent panel in the top-right corner during Flying state.
Shows the last 3-4 radio transmissions, fading older ones.

```
┌─ COM1: 120.5 ─────────────────────────────┐
│ SFO TWR: Ki-61 97B, cleared for the       │
│          option runway 28L                 │
│ 97B: Cleared for the option 28L, 97B      │
│ NorCal: Ki-61 42A, radar contact           │
└────────────────────────────────────────────┘
```

Styling:
- Background: FSBLUE with ~80% opacity
- Controller text: slightly brighter white or light cyan
- Pilot text: slightly dimmer, light gray
- Frequency header: small, top of panel
- Messages fade out after 10-15 seconds
- Show timestamp relative to sim time (optional, or just show most recent)
- Monospace font if available (fits the cockpit instrument aesthetic)
- Panel width: ~350-400px, right-aligned
- Don't show frequency tuning UI yet — just display COM1 as whatever the most "interesting" local frequency is (SFO Tower if near SFO, NorCal Approach otherwise)

Auto-tune logic for eavesdrop mode: pick the frequency with the most recent traffic
near the player's position. This means the player "hears" the most relevant chatter
without manually tuning.

---

## Display: Terminal Radio Log (ratatui)

Add a new panel to the telemetry dashboard, below the existing flight data panels.
Shows a scrolling log of all radio transmissions on all frequencies.

```
┌─ Radio ──────────────────────────────────────────────────┐
│ 120.5  SFO TWR  Ki-61 97B cleared for the option 28L    │
│ 120.5  97B      Cleared for the option 28L              │
│ 135.6  NorCal   Ki-61 42A radar contact squawk 2451     │
│ 135.6  42A      NorCal Ki-61 42A level 2400             │
│ 120.5  SFO TWR  Ki-61 97B report midfield downwind      │
└──────────────────────────────────────────────────────────┘
```

Format: `{freq}  {speaker_short}  {text_condensed}`

The telemetry SharedTelemetry struct needs a new field:
```rust
pub radio_log: Vec<RadioLogEntry>,  // last ~20 entries
```

Where:
```rust
pub struct RadioLogEntry {
    pub frequency: f32,
    pub speaker: String,     // short: "SFO TWR", "97B", "NorCal"
    pub text: String,
}
```

---

## AI Traffic Modifications

### Dedicated Pattern Plane

Modify `ai_traffic.rs` so that AI plane index 0 is a dedicated touch-and-go plane at SFO:

- Fixed altitude: ~1500 ft (pattern altitude for SFO)
- Fixed speed: ~120 kts (pattern speed)
- NavState gets additional pattern states or the ATC flight phase drives its navigation
- Flies a left-hand pattern for runway 28L:
  - Upwind (departing 28L heading ~280°)
  - Crosswind turn (left to ~190°)
  - Downwind (heading ~100°, parallel to 28L, offset ~1nm south)
  - Base turn (left to ~010°)
  - Final (heading ~280°, aligned with 28L)
  - Touch-and-go at the runway → back to upwind
- This plane generates the most radio traffic

The other 4 planes keep their existing figure-8 behavior and are tagged as EnRoute
for ATC purposes. They get periodic NorCal Approach check-ins and occasional
traffic advisories.

### Callsign Assignment

In `AiTrafficManager::new()`, assign callsigns to each plane:
```rust
plane 0: Ki-61 97B  (pattern plane)
plane 1: Ki-61 42A
plane 2: Ki-61 66C
plane 3: Ki-61 31D
plane 4: Ki-61 58E
```

### ATC State Initialization

Each plane starts with:
- Squawk: 2401 + plane_index (so 2401, 2402, 2403, 2404, 2405)
- Frequency: 135.65 (NorCal Approach) for en-route planes, 120.5 (SFO Tower) for pattern plane
- Flight phase: EnRoute for planes 1-4, Downwind for plane 0

---

## Ambient / Filler Transmissions

To keep the radio feeling alive during quiet periods, the AtcManager can inject
occasional ambient transmissions that aren't tied to any visible AI plane. These
represent traffic outside the player's visual range:

```
"NorCal Approach, Cessna three-two-six-papa-delta, request flight following to Sacramento"
"Cessna three-two-six-papa-delta, NorCal Approach, squawk four-two-seven-one, say altitude"
"United four-twelve heavy, NorCal Approach, descend and maintain three thousand"
"Descending three thousand, United four-twelve heavy"
```

Use a pool of ~10-15 fake callsigns (mix of GA and airline):
- Cessna + N-number style
- Skylane + N-number
- United / Southwest / Alaska + flight number
- Bonanza, Cherokee, Cirrus + N-number

These fire every 30–90 seconds during gaps. They add background texture.
Generate them with random but plausible content (altitude assignments,
heading changes, frequency changes, traffic advisories).

---

## Implementation Notes

- **phraseology.rs should be data-driven.** Template strings with placeholders,
  not a giant match statement. Makes it easy to add more message types later.
- **Timing is everything for realism.** Real radio has natural rhythm — a call,
  a pause, a response. Get the delays right and it'll feel authentic even with
  simple content. Get them wrong (instant responses, machine-gun pacing) and it
  sounds like a chatbot.
- **Don't block the main thread.** Message generation is cheap (string formatting),
  so it can run synchronously in the game loop. No need for async/threads.
  The 1Hz tick rate means negligible CPU cost.
- **Seeded RNG** for the ambient transmissions too. Same radio chatter every run
  during development, randomize later if desired.
- **Player frequency auto-tuning:** For now, the "COM1" display frequency is
  automatically set to the most relevant frequency based on player position.
  Within 10nm of SFO → SFO Tower. Otherwise → NorCal Approach. Show all
  messages on the terminal log regardless of frequency.
- **Future TTS hooks:** Each RadioMessage should have enough metadata that a
  future TTS system can pick the right voice (controller = calm/authoritative,
  pilot = varied). The `Speaker` enum already distinguishes this. Add a
  `voice_id: u8` field for future use.

---

## Dependencies

No new dependencies. Uses existing: `rand` (from ai_traffic), `egui` (from menu),
`ratatui`/`crossterm` (from telemetry).

---

## Do NOT

- Modify `renderer.rs` or shaders
- Implement player ATC participation (future work)
- Implement frequency tuning UI (future work)
- Model ATIS, ground control, or clearance delivery
- Use real airline callsigns that could cause trademark issues (United/Southwest are fine
  for a personal project, but note this if it ever goes public)
- Over-engineer the scheduling — a VecDeque with timestamps is sufficient
- Generate transmissions faster than one every 4-5 seconds (sounds like an auction house)

---

## Verification

After implementation, `cargo run --release -- -i` should show:
- Radio text appearing in the top-right overlay during flight
- Radio log scrolling in the terminal dashboard
- Pattern plane (97B) generating tower calls as it circuits SFO
- En-route planes getting occasional NorCal Approach check-ins
- Ambient airline/GA callsigns filling quiet gaps
- All transmissions using correct FAA digit pronunciation ("niner", not "nine")
- Realistic pacing: ~1 transmission every 5-15 seconds, with natural call/response pairs
- No transmission spam (minimum gaps enforced)