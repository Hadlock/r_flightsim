/// ATC Radio Chatter system.
///
/// Generates authentic FAA-phraseology radio transmissions for AI traffic
/// operating around SF Bay Area airports. The player eavesdrops on radio traffic.

pub mod facilities;
pub mod phraseology;
pub mod types;

use std::collections::VecDeque;

use glam::DVec3;
use rand::prelude::*;
use rand::rngs::StdRng;

use crate::ai_traffic::{AiPlane, NavState};
use crate::coords;

use facilities::AtcFacility;
use phraseology::*;
use types::*;

/// Minimum seconds between transmission pairs from the same plane.
const MIN_PLANE_INTERVAL: f64 = 20.0;
/// Minimum seconds between any two transmissions system-wide.
const MIN_GLOBAL_INTERVAL: f64 = 4.5;
/// Delay between pilot call and controller response.
const RESPONSE_DELAY_MIN: f64 = 1.5;
const RESPONSE_DELAY_MAX: f64 = 4.0;
/// Periodic en-route check-in interval range.
const ENROUTE_CHECKIN_MIN: f64 = 60.0;
const ENROUTE_CHECKIN_MAX: f64 = 120.0;
/// Ambient filler interval range.
const AMBIENT_MIN: f64 = 30.0;
const AMBIENT_MAX: f64 = 90.0;
/// Pattern leg duration estimates (seconds).
const PATTERN_DOWNWIND_DUR: f64 = 25.0;
const PATTERN_BASE_DUR: f64 = 15.0;
const PATTERN_FINAL_DUR: f64 = 20.0;
const PATTERN_TOUCHANDGO_DUR: f64 = 8.0;
const PATTERN_CROSSWIND_DUR: f64 = 15.0;
const PATTERN_DEPARTURE_DUR: f64 = 20.0;

/// SFO Tower frequency.
const SFO_FREQ: f32 = 120.5;
/// NorCal Approach frequency.
const NORCAL_FREQ: f32 = 135.65;

/// Distance threshold for "near SFO" auto-tune (meters, ~10nm).
const SFO_AUTOTUNE_DIST: f64 = 18_520.0;
/// SFO position for distance check.
const SFO_LAT: f64 = 37.6213;
const SFO_LON: f64 = -122.3790;

pub struct AtcManager {
    pub facilities: Vec<AtcFacility>,
    message_queue: VecDeque<RadioMessage>,  // scheduled upcoming messages
    message_log: VecDeque<RadioMessage>,    // delivered messages
    max_log_size: usize,
    sim_time: f64,
    last_global_transmission: f64,
    next_ambient_time: f64,
    /// Per-plane phase timers: how long in current pattern leg.
    pattern_timers: Vec<f64>,
    /// Per-plane en-route check-in timers.
    enroute_timers: Vec<f64>,
    rng: StdRng,
    /// Auto-tuned COM1 frequency for egui display.
    pub com1_freq: f32,
    /// TTS sender (None if TTS disabled).
    tts_sender: Option<crate::tts::TtsSender>,
}

impl AtcManager {
    pub fn new(num_planes: usize) -> Self {
        let mut rng = StdRng::seed_from_u64(0xA7C0);
        let next_ambient = rng.gen_range(AMBIENT_MIN..AMBIENT_MAX);

        // Initialize en-route timers with staggered offsets so they don't all talk at once
        let enroute_timers: Vec<f64> = (0..num_planes)
            .map(|i| {
                if i == 0 {
                    0.0 // pattern plane, not used for en-route
                } else {
                    // Stagger: plane 1 at 5s, plane 2 at 15s, etc.
                    5.0 + (i as f64 - 1.0) * 12.0
                }
            })
            .collect();

        // Pattern plane starts mid-downwind
        let mut pattern_timers = vec![0.0; num_planes];
        pattern_timers[0] = PATTERN_DOWNWIND_DUR * 0.3; // partway through downwind

        AtcManager {
            facilities: facilities::build_facilities(),
            message_queue: VecDeque::new(),
            message_log: VecDeque::new(),
            max_log_size: 50,
            sim_time: 0.0,
            last_global_transmission: -10.0, // allow immediate first message
            next_ambient_time: next_ambient,
            pattern_timers,
            enroute_timers,
            rng,
            com1_freq: NORCAL_FREQ,
            tts_sender: None,
        }
    }

    /// Advance the ATC system. Call once per frame (internally rate-limits).
    pub fn tick(
        &mut self,
        dt: f64,
        planes: &[AiPlane],
        atc_states: &mut [AiPlaneAtcState],
        player_pos: DVec3,
    ) {
        self.sim_time += dt;

        // Deliver scheduled messages whose time has arrived
        while let Some(msg) = self.message_queue.front() {
            if msg.timestamp <= self.sim_time {
                let msg = self.message_queue.pop_front().unwrap();
                // Send to TTS
                if let Some(ref tts) = self.tts_sender {
                    tts.send(msg.voice_id, &msg.text);
                }
                self.message_log.push_back(msg);
                while self.message_log.len() > self.max_log_size {
                    self.message_log.pop_front();
                }
            } else {
                break;
            }
        }

        // Update pattern timers
        for i in 0..planes.len().min(self.pattern_timers.len()) {
            self.pattern_timers[i] += dt;
        }

        // Generate transmissions for each plane
        for i in 0..planes.len().min(atc_states.len()) {
            if !self.can_transmit(i, atc_states) {
                continue;
            }

            let msgs = self.generate_for_plane(i, &planes[i], &mut atc_states[i]);
            if !msgs.is_empty() {
                atc_states[i].last_transmission = self.sim_time;
                self.last_global_transmission = self.sim_time;
                for msg in msgs {
                    self.message_queue.push_back(msg);
                }
            }
        }

        // Ambient filler
        if self.sim_time >= self.next_ambient_time
            && self.sim_time - self.last_global_transmission >= MIN_GLOBAL_INTERVAL
        {
            let msgs = self.generate_ambient();
            if !msgs.is_empty() {
                self.last_global_transmission = self.sim_time;
                for msg in msgs {
                    self.message_queue.push_back(msg);
                }
            }
            self.next_ambient_time =
                self.sim_time + self.rng.gen_range(AMBIENT_MIN..AMBIENT_MAX);
        }

        // Auto-tune COM1 based on player position
        self.update_com1(player_pos);
    }

    /// Check if plane i is allowed to transmit right now.
    fn can_transmit(&self, plane_idx: usize, atc_states: &[AiPlaneAtcState]) -> bool {
        let state = &atc_states[plane_idx];
        let since_last = self.sim_time - state.last_transmission;
        let since_global = self.sim_time - self.last_global_transmission;

        since_last >= MIN_PLANE_INTERVAL && since_global >= MIN_GLOBAL_INTERVAL
    }

    /// Generate transmissions for a specific plane based on its flight phase.
    fn generate_for_plane(
        &mut self,
        idx: usize,
        plane: &AiPlane,
        atc_state: &mut AiPlaneAtcState,
    ) -> Vec<RadioMessage> {
        match atc_state.flight_phase {
            FlightPhase::EnRoute => self.generate_enroute(idx, plane, atc_state),
            FlightPhase::Downwind => self.generate_downwind(idx, plane, atc_state),
            FlightPhase::Base => self.generate_base(idx, plane, atc_state),
            FlightPhase::Final => self.generate_final(idx, plane, atc_state),
            FlightPhase::TouchAndGo => self.generate_touchandgo(idx, plane, atc_state),
            FlightPhase::Crosswind => self.generate_crosswind(idx, plane, atc_state),
            FlightPhase::Departure => self.generate_departure(idx, plane, atc_state),
        }
    }

    // --- En-Route messages (planes 1-4 doing figure-8s) ---

    fn generate_enroute(
        &mut self,
        idx: usize,
        plane: &AiPlane,
        atc_state: &mut AiPlaneAtcState,
    ) -> Vec<RadioMessage> {
        let timer = self.enroute_timers.get(idx).copied().unwrap_or(0.0);

        if !atc_state.initial_contact_made {
            atc_state.initial_contact_made = true;
            self.enroute_timers[idx] = 0.0;
            return self.enroute_initial_contact(idx, plane, atc_state);
        }

        // Periodic check-in
        if timer >= ENROUTE_CHECKIN_MIN
            && (timer >= ENROUTE_CHECKIN_MAX
                || self.rng.gen_bool(0.02)) // ~2% chance per tick after min interval
        {
            self.enroute_timers[idx] = 0.0;

            // Handoff check: is this plane near an airport?
            if matches!(plane.nav_state(), NavState::Transit) {
                // Just do a periodic check-in
                return self.enroute_checkin(idx, plane, atc_state);
            }

            return self.enroute_checkin(idx, plane, atc_state);
        }

        // Advance timer
        // (timer already advanced in tick via pattern_timers, but en-route uses separate timer)
        // Actually, we track en-route timers separately
        if idx < self.enroute_timers.len() {
            // dt is implicit from tick rate, but we accumulate differently
            // The timer is advanced once we check, reset to 0 on transmission
        }

        vec![]
    }

    fn enroute_initial_contact(
        &mut self,
        idx: usize,
        plane: &AiPlane,
        atc_state: &AiPlaneAtcState,
    ) -> Vec<RadioMessage> {
        let cs_full = atc_state.callsign.full();
        let alt = speak_altitude(plane.altitude_ft());
        let wp_name = nearest_waypoint_name(plane);
        let delay = self.rng.gen_range(RESPONSE_DELAY_MIN..RESPONSE_DELAY_MAX);

        let pilot_text = format!(
            "NorCal Approach, {}, {} feet, proceeding direct {}",
            cs_full, alt, wp_name
        );
        let atc_text = format!(
            "{}, NorCal Approach, radar contact, squawk {}",
            atc_state.callsign.display_full(),
            speak_squawk(atc_state.squawk)
        );

        vec![
            RadioMessage {
                timestamp: self.sim_time,
                frequency: NORCAL_FREQ,
                speaker: Speaker::Pilot(idx),
                text: pilot_text,
                display_speaker: atc_state.callsign.display_short(),
                voice_id: idx as u8,
            },
            RadioMessage {
                timestamp: self.sim_time + delay,
                frequency: NORCAL_FREQ,
                speaker: Speaker::Controller("NorCal Approach".to_string()),
                text: atc_text,
                display_speaker: "NorCal".to_string(),
                voice_id: 100,
            },
        ]
    }

    fn enroute_checkin(
        &mut self,
        idx: usize,
        plane: &AiPlane,
        atc_state: &AiPlaneAtcState,
    ) -> Vec<RadioMessage> {
        let alt = speak_altitude(plane.altitude_ft());
        let delay = self.rng.gen_range(RESPONSE_DELAY_MIN..RESPONSE_DELAY_MAX);

        let template = self.rng.gen_range(0u8..3);
        match template {
            0 => {
                // Simple level report
                vec![
                    RadioMessage {
                        timestamp: self.sim_time,
                        frequency: NORCAL_FREQ,
                        speaker: Speaker::Pilot(idx),
                        text: format!(
                            "NorCal Approach, {}, level {}",
                            atc_state.callsign.full(), alt
                        ),
                        display_speaker: atc_state.callsign.display_short(),
                        voice_id: idx as u8,
                    },
                    RadioMessage {
                        timestamp: self.sim_time + delay,
                        frequency: NORCAL_FREQ,
                        speaker: Speaker::Controller("NorCal Approach".to_string()),
                        text: format!("{}, roger", atc_state.callsign.display_full()),
                        display_speaker: "NorCal".to_string(),
                        voice_id: 100,
                    },
                ]
            }
            1 => {
                // Traffic advisory
                let clock = clock_position(self.rng.gen_range(0.0..360.0));
                let dist = self.rng.gen_range(3..12);
                let traf_alt = speak_altitude(
                    (self.rng.gen_range(10..50) * 100) as f64,
                );
                vec![
                    RadioMessage {
                        timestamp: self.sim_time,
                        frequency: NORCAL_FREQ,
                        speaker: Speaker::Controller("NorCal Approach".to_string()),
                        text: format!(
                            "{}, traffic {}, {} miles, {}, type unknown",
                            atc_state.callsign.display_full(), clock, dist, traf_alt
                        ),
                        display_speaker: "NorCal".to_string(),
                        voice_id: 100,
                    },
                    RadioMessage {
                        timestamp: self.sim_time + delay,
                        frequency: NORCAL_FREQ,
                        speaker: Speaker::Pilot(idx),
                        text: format!("Looking for traffic, {}", atc_state.callsign.short()),
                        display_speaker: atc_state.callsign.display_short(),
                        voice_id: idx as u8,
                    },
                ]
            }
            _ => {
                // Altimeter update
                vec![
                    RadioMessage {
                        timestamp: self.sim_time,
                        frequency: NORCAL_FREQ,
                        speaker: Speaker::Controller("NorCal Approach".to_string()),
                        text: format!(
                            "{}, altimeter {}",
                            atc_state.callsign.display_full(), speak_altimeter()
                        ),
                        display_speaker: "NorCal".to_string(),
                        voice_id: 100,
                    },
                    RadioMessage {
                        timestamp: self.sim_time + delay,
                        frequency: NORCAL_FREQ,
                        speaker: Speaker::Pilot(idx),
                        text: format!(
                            "{}, {}",
                            speak_altimeter(), atc_state.callsign.short()
                        ),
                        display_speaker: atc_state.callsign.display_short(),
                        voice_id: idx as u8,
                    },
                ]
            }
        }
    }

    // --- Pattern messages (plane 0 doing touch-and-go at SFO) ---

    fn generate_downwind(
        &mut self,
        idx: usize,
        _plane: &AiPlane,
        atc_state: &mut AiPlaneAtcState,
    ) -> Vec<RadioMessage> {
        let timer = self.pattern_timers[idx];
        if timer < PATTERN_DOWNWIND_DUR * 0.3 {
            return vec![];
        }

        let delay = self.rng.gen_range(RESPONSE_DELAY_MIN..RESPONSE_DELAY_MAX);

        // Advance to base
        atc_state.flight_phase = FlightPhase::Base;
        self.pattern_timers[idx] = 0.0;

        let template = self.rng.gen_range(0u8..2);
        if template == 0 && !atc_state.cleared_option {
            // Cleared for the option
            atc_state.cleared_option = true;
            vec![
                RadioMessage {
                    timestamp: self.sim_time,
                    frequency: SFO_FREQ,
                    speaker: Speaker::Pilot(idx),
                    text: format!(
                        "San Francisco Tower, {}, left downwind runway two-eight left",
                        atc_state.callsign.full()
                    ),
                    display_speaker: atc_state.callsign.display_short(),
                    voice_id: idx as u8,
                },
                RadioMessage {
                    timestamp: self.sim_time + delay,
                    frequency: SFO_FREQ,
                    speaker: Speaker::Controller("SFO Tower".to_string()),
                    text: format!(
                        "{}, San Francisco Tower, cleared for the option runway two-eight left",
                        atc_state.callsign.display_full()
                    ),
                    display_speaker: "SFO TWR".to_string(),
                    voice_id: 101,
                },
                RadioMessage {
                    timestamp: self.sim_time + delay + 1.5,
                    frequency: SFO_FREQ,
                    speaker: Speaker::Pilot(idx),
                    text: format!(
                        "Cleared for the option two-eight left, {}",
                        atc_state.callsign.short()
                    ),
                    display_speaker: atc_state.callsign.display_short(),
                    voice_id: idx as u8,
                },
            ]
        } else {
            // Number in sequence
            let seq = self.rng.gen_range(1..4);
            vec![
                RadioMessage {
                    timestamp: self.sim_time,
                    frequency: SFO_FREQ,
                    speaker: Speaker::Pilot(idx),
                    text: format!(
                        "San Francisco Tower, {}, left downwind runway two-eight left",
                        atc_state.callsign.full()
                    ),
                    display_speaker: atc_state.callsign.display_short(),
                    voice_id: idx as u8,
                },
                RadioMessage {
                    timestamp: self.sim_time + delay,
                    frequency: SFO_FREQ,
                    speaker: Speaker::Controller("SFO Tower".to_string()),
                    text: format!(
                        "{}, number {}, follow traffic on base",
                        atc_state.callsign.display_full(), number_word_simple(seq)
                    ),
                    display_speaker: "SFO TWR".to_string(),
                    voice_id: 101,
                },
            ]
        }
    }

    fn generate_base(
        &mut self,
        idx: usize,
        _plane: &AiPlane,
        atc_state: &mut AiPlaneAtcState,
    ) -> Vec<RadioMessage> {
        let timer = self.pattern_timers[idx];
        if timer < PATTERN_BASE_DUR * 0.5 {
            return vec![];
        }

        atc_state.flight_phase = FlightPhase::Final;
        self.pattern_timers[idx] = 0.0;
        let delay = self.rng.gen_range(RESPONSE_DELAY_MIN..RESPONSE_DELAY_MAX);

        vec![
            RadioMessage {
                timestamp: self.sim_time,
                frequency: SFO_FREQ,
                speaker: Speaker::Pilot(idx),
                text: format!(
                    "{}, turning base",
                    atc_state.callsign.display_full()
                ),
                display_speaker: atc_state.callsign.display_short(),
                voice_id: idx as u8,
            },
            RadioMessage {
                timestamp: self.sim_time + delay,
                frequency: SFO_FREQ,
                speaker: Speaker::Controller("SFO Tower".to_string()),
                text: format!("{}, roger", atc_state.callsign.display_full()),
                display_speaker: "SFO TWR".to_string(),
                voice_id: 101,
            },
        ]
    }

    fn generate_final(
        &mut self,
        idx: usize,
        _plane: &AiPlane,
        atc_state: &mut AiPlaneAtcState,
    ) -> Vec<RadioMessage> {
        let timer = self.pattern_timers[idx];
        if timer < PATTERN_FINAL_DUR * 0.5 {
            return vec![];
        }

        atc_state.flight_phase = FlightPhase::TouchAndGo;
        self.pattern_timers[idx] = 0.0;
        let delay = self.rng.gen_range(RESPONSE_DELAY_MIN..RESPONSE_DELAY_MAX);
        let wind_speed = self.rng.gen_range(6..12);

        vec![
            RadioMessage {
                timestamp: self.sim_time,
                frequency: SFO_FREQ,
                speaker: Speaker::Pilot(idx),
                text: format!(
                    "{}, short final two-eight left",
                    atc_state.callsign.display_full()
                ),
                display_speaker: atc_state.callsign.display_short(),
                voice_id: idx as u8,
            },
            RadioMessage {
                timestamp: self.sim_time + delay,
                frequency: SFO_FREQ,
                speaker: Speaker::Controller("SFO Tower".to_string()),
                text: format!(
                    "{}, cleared touch and go runway two-eight left, wind two-seven-zero at {}",
                    atc_state.callsign.display_full(),
                    number_word_simple(wind_speed)
                ),
                display_speaker: "SFO TWR".to_string(),
                voice_id: 101,
            },
            RadioMessage {
                timestamp: self.sim_time + delay + 1.5,
                frequency: SFO_FREQ,
                speaker: Speaker::Pilot(idx),
                text: format!(
                    "Cleared touch and go, {}",
                    atc_state.callsign.short()
                ),
                display_speaker: atc_state.callsign.display_short(),
                voice_id: idx as u8,
            },
        ]
    }

    fn generate_touchandgo(
        &mut self,
        idx: usize,
        _plane: &AiPlane,
        atc_state: &mut AiPlaneAtcState,
    ) -> Vec<RadioMessage> {
        let timer = self.pattern_timers[idx];
        if timer < PATTERN_TOUCHANDGO_DUR {
            return vec![];
        }

        atc_state.flight_phase = FlightPhase::Crosswind;
        self.pattern_timers[idx] = 0.0;
        // No radio call during the actual touch-and-go roll
        vec![]
    }

    fn generate_crosswind(
        &mut self,
        idx: usize,
        _plane: &AiPlane,
        atc_state: &mut AiPlaneAtcState,
    ) -> Vec<RadioMessage> {
        let timer = self.pattern_timers[idx];
        if timer < PATTERN_CROSSWIND_DUR {
            return vec![];
        }

        atc_state.flight_phase = FlightPhase::Departure;
        self.pattern_timers[idx] = 0.0;
        let delay = self.rng.gen_range(RESPONSE_DELAY_MIN..RESPONSE_DELAY_MAX);

        vec![
            RadioMessage {
                timestamp: self.sim_time,
                frequency: SFO_FREQ,
                speaker: Speaker::Controller("SFO Tower".to_string()),
                text: format!(
                    "{}, make left crosswind departure, contact NorCal on {}",
                    atc_state.callsign.display_full(),
                    speak_frequency(NORCAL_FREQ)
                ),
                display_speaker: "SFO TWR".to_string(),
                voice_id: 101,
            },
            RadioMessage {
                timestamp: self.sim_time + delay,
                frequency: SFO_FREQ,
                speaker: Speaker::Pilot(idx),
                text: format!(
                    "Left crosswind, NorCal on {}, {}",
                    speak_frequency(NORCAL_FREQ),
                    atc_state.callsign.short()
                ),
                display_speaker: atc_state.callsign.display_short(),
                voice_id: idx as u8,
            },
        ]
    }

    fn generate_departure(
        &mut self,
        idx: usize,
        _plane: &AiPlane,
        atc_state: &mut AiPlaneAtcState,
    ) -> Vec<RadioMessage> {
        let timer = self.pattern_timers[idx];
        if timer < PATTERN_DEPARTURE_DUR {
            return vec![];
        }

        // Re-enter downwind, reset cleared_option for next circuit
        atc_state.flight_phase = FlightPhase::Downwind;
        atc_state.cleared_option = false;
        self.pattern_timers[idx] = 0.0;
        let delay = self.rng.gen_range(RESPONSE_DELAY_MIN..RESPONSE_DELAY_MAX);

        vec![
            RadioMessage {
                timestamp: self.sim_time,
                frequency: SFO_FREQ,
                speaker: Speaker::Pilot(idx),
                text: format!(
                    "San Francisco Tower, {}, re-entering left downwind two-eight left, one thousand five hundred",
                    atc_state.callsign.full()
                ),
                display_speaker: atc_state.callsign.display_short(),
                voice_id: idx as u8,
            },
            RadioMessage {
                timestamp: self.sim_time + delay,
                frequency: SFO_FREQ,
                speaker: Speaker::Controller("SFO Tower".to_string()),
                text: format!(
                    "{}, report midfield downwind",
                    atc_state.callsign.display_full()
                ),
                display_speaker: "SFO TWR".to_string(),
                voice_id: 101,
            },
        ]
    }

    // --- Ambient filler ---

    fn generate_ambient(&mut self) -> Vec<RadioMessage> {
        let (pilot_text, pilot_display, atc_text, atc_display) =
            generate_ambient_pair(&mut self.rng);

        let delay = self.rng.gen_range(RESPONSE_DELAY_MIN..RESPONSE_DELAY_MAX);
        let freq = if self.rng.gen_bool(0.6) {
            NORCAL_FREQ
        } else {
            SFO_FREQ
        };

        vec![
            RadioMessage {
                timestamp: self.sim_time,
                frequency: freq,
                speaker: Speaker::Ambient,
                text: pilot_text,
                display_speaker: pilot_display,
                voice_id: 200,
            },
            RadioMessage {
                timestamp: self.sim_time + delay,
                frequency: freq,
                speaker: Speaker::Ambient,
                text: atc_text,
                display_speaker: atc_display,
                voice_id: 201,
            },
        ]
    }

    // --- Auto-tune COM1 ---

    fn update_com1(&mut self, player_pos: DVec3) {
        let player_lla = coords::ecef_to_lla(player_pos);
        let sfo_ecef = coords::lla_to_ecef(&coords::LLA {
            lat: SFO_LAT.to_radians(),
            lon: SFO_LON.to_radians(),
            alt: 0.0,
        });
        let dist = (player_pos - sfo_ecef).length();
        self.com1_freq = if dist < SFO_AUTOTUNE_DIST {
            SFO_FREQ
        } else {
            NORCAL_FREQ
        };
        let _ = player_lla; // suppress unused warning
    }

    // --- Public accessors ---

    /// Get recent messages for display (last N seconds).
    pub fn recent_messages(&self, seconds: f64) -> Vec<&RadioMessage> {
        self.message_log
            .iter()
            .filter(|m| self.sim_time - m.timestamp < seconds)
            .collect()
    }

    /// Get the most recent message.
    pub fn latest_message(&self) -> Option<&RadioMessage> {
        self.message_log.back()
    }

    /// Get all messages in the log (for telemetry).
    pub fn message_log(&self) -> &VecDeque<RadioMessage> {
        &self.message_log
    }

    /// Set the TTS sender for speech synthesis.
    pub fn set_tts_sender(&mut self, sender: crate::tts::TtsSender) {
        self.tts_sender = Some(sender);
    }

    /// Advance en-route timers (called each tick with dt).
    pub fn advance_enroute_timers(&mut self, dt: f64) {
        for timer in &mut self.enroute_timers {
            *timer += dt;
        }
    }
}

/// Get nearest waypoint name for an en-route plane.
fn nearest_waypoint_name(plane: &AiPlane) -> &'static str {
    match plane.current_waypoint() {
        0 => "San Francisco",
        1 => "Emeryville",
        2 => "Golden Gate",
        _ => "Bay Area",
    }
}

/// Simple number word for small numbers (1-12).
fn number_word_simple(n: u32) -> &'static str {
    match n {
        1 => "one",
        2 => "two",
        3 => "three",
        4 => "four",
        5 => "five",
        6 => "six",
        7 => "seven",
        8 => "eight",
        9 => "niner",
        10 => "ten",
        11 => "eleven",
        12 => "twelve",
        _ => "?",
    }
}

// --- Callsign definitions for the 5+ AI planes ---

/// Build ATC state for plane at given index. Per CLAUDE.md spec:
/// plane 0: Ki-61 97B (pattern), planes 1-4: Ki-61 42A/66C/31D/58E (en-route)
/// Additional planes (5-6) get generated callsigns.
pub fn build_atc_state(plane_idx: usize) -> AiPlaneAtcState {
    let (tail_number, tail_phonetic) = match plane_idx {
        0 => ("97B", "niner-seven-bravo"),
        1 => ("42A", "four-two-alpha"),
        2 => ("66C", "six-six-charlie"),
        3 => ("31D", "three-one-delta"),
        4 => ("58E", "five-eight-echo"),
        5 => ("14F", "one-four-foxtrot"),
        6 => ("73G", "seven-three-golf"),
        _ => ("99X", "niner-niner-x-ray"),
    };

    let is_pattern_plane = plane_idx == 0;

    AiPlaneAtcState {
        callsign: Callsign {
            aircraft_type: "Ki-61",
            tail_number,
            tail_phonetic,
        },
        squawk: 2401 + plane_idx as u16,
        current_freq: if is_pattern_plane { SFO_FREQ } else { NORCAL_FREQ },
        flight_phase: if is_pattern_plane {
            FlightPhase::Downwind
        } else {
            FlightPhase::EnRoute
        },
        last_transmission: -30.0, // allow initial transmission soon
        initial_contact_made: false,
        cleared_option: false,
    }
}
