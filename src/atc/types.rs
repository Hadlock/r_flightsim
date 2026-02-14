/// Shared types for the ATC radio chatter system.

/// Callsign: aircraft type + tail number.
#[derive(Clone, Debug)]
pub struct Callsign {
    pub aircraft_type: &'static str, // "Ki-61"
    pub tail_number: &'static str,   // "97B", "42A", etc.
    pub tail_phonetic: &'static str, // "niner-seven-bravo"
}

impl Callsign {
    /// Full callsign: "Ki-61 niner-seven-bravo"
    pub fn full(&self) -> String {
        format!("{} {}", self.aircraft_type, self.tail_phonetic)
    }

    /// Short callsign (after initial contact): "niner-seven-bravo"
    pub fn short(&self) -> String {
        self.tail_phonetic.to_string()
    }

    /// Display-friendly short: "97B"
    pub fn display_short(&self) -> String {
        self.tail_number.to_string()
    }

    /// Display-friendly full: "Ki-61 97B"
    pub fn display_full(&self) -> String {
        format!("{} {}", self.aircraft_type, self.tail_number)
    }
}

/// Who is speaking on the radio.
#[derive(Clone, Debug)]
pub enum Speaker {
    Pilot(usize),     // AI plane index
    Controller(String), // facility name, e.g. "SFO Tower"
    Ambient,           // background traffic (not tied to visible AI plane)
}

/// A single radio transmission.
#[derive(Clone, Debug)]
pub struct RadioMessage {
    pub timestamp: f64,       // sim time when this should be heard
    pub frequency: f32,       // MHz
    pub speaker: Speaker,
    pub text: String,         // the spoken text (FAA phraseology)
    pub display_speaker: String, // short speaker label for display: "SFO TWR", "97B", "NorCal"
    pub voice_id: u8,         // future TTS hook
}

/// Flight phase drives what ATC messages are generated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlightPhase {
    // En-route planes (figure-8 between waypoints, on NorCal Approach)
    EnRoute,

    // Pattern plane (touch-and-go at SFO)
    Downwind,
    Base,
    Final,
    TouchAndGo,
    Crosswind,
    Departure,
}

/// Per-plane ATC state, attached to each AI plane.
#[derive(Clone, Debug)]
pub struct AiPlaneAtcState {
    pub callsign: Callsign,
    pub squawk: u16,
    pub current_freq: f32,
    pub flight_phase: FlightPhase,
    pub last_transmission: f64,   // sim time of last radio call
    pub initial_contact_made: bool, // whether initial contact with ATC has been done
    pub cleared_option: bool,       // whether cleared for the option (pattern plane)
}

/// Entry for the telemetry radio log display.
#[derive(Clone, Debug)]
pub struct RadioLogEntry {
    pub frequency: f32,
    pub speaker: String,  // short: "SFO TWR", "97B", "NorCal"
    pub text: String,
}
