/// FAA phraseology formatting and message templates.

use rand::prelude::*;
use rand::rngs::StdRng;

// --- Digit pronunciation ---

/// Pronounce a single digit per FAA convention.
fn digit_word(d: u8) -> &'static str {
    match d {
        0 => "zero",
        1 => "one",
        2 => "two",
        3 => "three",
        4 => "four",
        5 => "five",
        6 => "six",
        7 => "seven",
        8 => "eight",
        9 => "niner",
        _ => "?",
    }
}

/// Speak digits individually: "2451" -> "two-four-five-one"
pub fn speak_digits(s: &str) -> String {
    s.chars()
        .filter_map(|c| c.to_digit(10).map(|d| digit_word(d as u8)))
        .collect::<Vec<_>>()
        .join("-")
}

/// Speak a squawk code: 2451 -> "two-four-five-one"
pub fn speak_squawk(code: u16) -> String {
    speak_digits(&format!("{:04}", code))
}

// --- Altitude ---

/// Speak altitude in hundreds/thousands: 2400 -> "two thousand four hundred"
pub fn speak_altitude(feet: f64) -> String {
    let feet = (feet / 100.0).round() as i32 * 100; // round to nearest 100
    let feet = feet.max(0);

    let thousands = feet / 1000;
    let hundreds = (feet % 1000) / 100;

    match (thousands, hundreds) {
        (0, 0) => "field elevation".to_string(),
        (0, h) => format!("{} hundred", number_word(h)),
        (t, 0) => format!("{} thousand", number_word(t)),
        (t, 5) => format!("{} thousand five hundred", number_word(t)),
        (t, h) => format!("{} thousand {} hundred", number_word(t), number_word(h)),
    }
}

fn number_word(n: i32) -> &'static str {
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

// --- Heading ---

/// Speak a heading: 280 -> "two-eight-zero"
pub fn speak_heading(hdg: f64) -> String {
    let hdg = ((hdg % 360.0 + 360.0) % 360.0).round() as u32;
    let d1 = (hdg / 100) as u8;
    let d2 = ((hdg / 10) % 10) as u8;
    let d3 = (hdg % 10) as u8;
    format!("{}-{}-{}", digit_word(d1), digit_word(d2), digit_word(d3))
}

// --- Runway ---

/// Speak a runway designator: "28L" -> "two-eight left"
pub fn speak_runway(rwy: &str) -> String {
    let mut result = Vec::new();
    for c in rwy.chars() {
        if let Some(d) = c.to_digit(10) {
            result.push(digit_word(d as u8).to_string());
        } else {
            let suffix = match c {
                'L' => "left",
                'R' => "right",
                'C' => "center",
                _ => "",
            };
            if !suffix.is_empty() {
                result.push(suffix.to_string());
            }
        }
    }
    result.join(" ")
}

// --- Frequency ---

/// Speak a frequency: 120.5 -> "one-two-zero-point-five"
/// Also supports informal: 135.65 -> "thirty-five sixty-five"
pub fn speak_frequency(freq: f32) -> String {
    // Use informal shorthand for common frequencies
    let mhz = (freq * 100.0).round() as u32;
    let whole = mhz / 100;
    let frac = mhz % 100;

    if whole >= 118 && whole <= 136 {
        // Informal shorthand: strip leading "1", speak two groups
        let last_two = whole % 100;
        if frac == 0 {
            format!("{}-{}-point-zero", digit_word((whole / 100) as u8),
                    speak_digits(&format!("{:02}", last_two)))
        } else if frac % 10 == 0 {
            // e.g., 135.60 -> "thirty-five sixty"
            let group1 = last_two;
            let group2 = frac;
            format!("{} {}", speak_two_digit(group1), speak_two_digit(group2))
        } else {
            // Full pronunciation
            let s = format!("{:.2}", freq);
            let parts: Vec<&str> = s.split('.').collect();
            format!("{}-point-{}", speak_digits(parts[0]), speak_digits(parts[1]))
        }
    } else {
        let s = format!("{:.1}", freq);
        let parts: Vec<&str> = s.split('.').collect();
        format!("{}-point-{}", speak_digits(parts[0]), speak_digits(parts[1]))
    }
}

/// Speak a two-digit number naturally: 35 -> "thirty-five", 20 -> "twenty"
fn speak_two_digit(n: u32) -> String {
    let tens = n / 10;
    let ones = n % 10;
    let tens_word = match tens {
        1 => return match ones {
            0 => "ten".to_string(),
            1 => "eleven".to_string(),
            2 => "twelve".to_string(),
            3 => "thirteen".to_string(),
            4 => "fourteen".to_string(),
            5 => "fifteen".to_string(),
            6 => "sixteen".to_string(),
            7 => "seventeen".to_string(),
            8 => "eighteen".to_string(),
            9 => "nineteen".to_string(),
            _ => "?".to_string(),
        },
        2 => "twenty",
        3 => "thirty",
        4 => "forty",
        5 => "fifty",
        6 => "sixty",
        7 => "seventy",
        8 => "eighty",
        9 => "ninety",
        0 => {
            return if ones == 0 {
                "zero".to_string()
            } else {
                digit_word(ones as u8).to_string()
            };
        }
        _ => "?",
    };

    if ones == 0 {
        tens_word.to_string()
    } else {
        format!("{}-{}", tens_word, digit_word(ones as u8))
    }
}

// --- Wind ---

/// Fixed wind for now: "two-seven-zero at eight"
pub fn speak_wind() -> &'static str {
    "two-seven-zero at eight"
}

// --- Altimeter ---

/// Fixed altimeter: "three-zero-one-niner"
pub fn speak_altimeter() -> &'static str {
    "three-zero-one-niner"
}

// --- Compass direction ---

/// Cardinal direction from heading: "north", "northeast", etc.
pub fn compass_direction(bearing_deg: f64) -> &'static str {
    let b = ((bearing_deg % 360.0) + 360.0) % 360.0;
    match b as u32 {
        338..=360 | 0..=22 => "north",
        23..=67 => "northeast",
        68..=112 => "east",
        113..=157 => "southeast",
        158..=202 => "south",
        203..=247 => "southwest",
        248..=292 => "west",
        293..=337 => "northwest",
        _ => "north",
    }
}

/// Clock position from relative bearing: "twelve o'clock", "three o'clock", etc.
pub fn clock_position(relative_bearing_deg: f64) -> &'static str {
    let b = ((relative_bearing_deg % 360.0) + 360.0) % 360.0;
    let hour = ((b + 15.0) / 30.0) as u32 % 12;
    match hour {
        0 => "twelve o'clock",
        1 => "one o'clock",
        2 => "two o'clock",
        3 => "three o'clock",
        4 => "four o'clock",
        5 => "five o'clock",
        6 => "six o'clock",
        7 => "seven o'clock",
        8 => "eight o'clock",
        9 => "nine o'clock",
        10 => "ten o'clock",
        11 => "eleven o'clock",
        _ => "twelve o'clock",
    }
}

// --- Ambient callsign pool ---

pub struct AmbientCallsign {
    pub spoken: &'static str,
    pub display: &'static str,
}

/// Pool of ambient callsigns for filler transmissions.
pub fn ambient_callsigns() -> &'static [AmbientCallsign] {
    &[
        AmbientCallsign { spoken: "Cessna three-two-six-papa-delta", display: "C326PD" },
        AmbientCallsign { spoken: "Skylane niner-one-four-tango", display: "N914T" },
        AmbientCallsign { spoken: "United four-twelve heavy", display: "UAL412" },
        AmbientCallsign { spoken: "Southwest two-niner-eight", display: "SWA298" },
        AmbientCallsign { spoken: "Alaska six-five-one", display: "ASA651" },
        AmbientCallsign { spoken: "Bonanza eight-three-niner-charlie", display: "N839C" },
        AmbientCallsign { spoken: "Cherokee five-five-two-alpha", display: "N552A" },
        AmbientCallsign { spoken: "Cirrus seven-eight-delta-echo", display: "N78DE" },
        AmbientCallsign { spoken: "Cessna one-four-seven-bravo", display: "N147B" },
        AmbientCallsign { spoken: "Skylane six-niner-two-tango-mike", display: "N692TM" },
        AmbientCallsign { spoken: "United eight-seven-three heavy", display: "UAL873" },
        AmbientCallsign { spoken: "Alaska three-two-seven", display: "ASA327" },
    ]
}

// --- Ambient message templates ---

/// Generate a pair of ambient transmissions (pilot + controller).
/// Returns (pilot_text, pilot_display, controller_text, controller_display).
pub fn generate_ambient_pair(rng: &mut StdRng) -> (String, String, String, String) {
    let callsigns = ambient_callsigns();
    let cs = &callsigns[rng.gen_range(0..callsigns.len())];

    let alt_hundreds: u32 = rng.gen_range(15..80) * 100;
    let alt_spoken = speak_altitude(alt_hundreds as f64);

    let template = rng.gen_range(0u8..5);
    match template {
        0 => {
            // Flight following request
            let dest = ["Sacramento", "Monterey", "Santa Rosa", "Stockton"][rng.gen_range(0..4)];
            (
                format!("NorCal Approach, {}, request flight following to {}", cs.spoken, dest),
                cs.display.to_string(),
                format!("{}, NorCal Approach, squawk {}, say altitude",
                        cs.spoken, speak_squawk(rng.gen_range(2000..5000))),
                "NorCal".to_string(),
            )
        }
        1 => {
            // Altitude assignment
            let new_alt = rng.gen_range(20..60) * 100;
            (
                format!("{}, NorCal Approach, descend and maintain {}",
                        cs.spoken, speak_altitude(new_alt as f64)),
                "NorCal".to_string(),
                format!("Descending {}, {}", speak_altitude(new_alt as f64), cs.spoken),
                cs.display.to_string(),
            )
        }
        2 => {
            // Heading change
            let hdg = rng.gen_range(0..36) * 10;
            (
                format!("{}, turn right heading {}", cs.spoken, speak_heading(hdg as f64)),
                "NorCal".to_string(),
                format!("Right heading {}, {}", speak_heading(hdg as f64), cs.spoken),
                cs.display.to_string(),
            )
        }
        3 => {
            // Frequency change
            let freqs = [120.5, 118.3, 124.0, 118.6, 119.2];
            let names = ["San Francisco Tower", "Oakland Tower", "San Jose Tower",
                         "Palo Alto Tower", "San Carlos Tower"];
            let idx = rng.gen_range(0..freqs.len());
            (
                format!("{}, contact {} on {}", cs.spoken, names[idx], speak_frequency(freqs[idx])),
                "NorCal".to_string(),
                format!("{} on {}, {}", names[idx], speak_frequency(freqs[idx]), cs.spoken),
                cs.display.to_string(),
            )
        }
        _ => {
            // Position report
            (
                format!("NorCal Approach, {}, level {}", cs.spoken, alt_spoken),
                cs.display.to_string(),
                format!("{}, NorCal Approach, roger", cs.spoken),
                "NorCal".to_string(),
            )
        }
    }
}
