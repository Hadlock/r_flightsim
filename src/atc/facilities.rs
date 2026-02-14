/// ATC facility definitions for Bay Area airports.

pub enum FacilityType {
    Approach,
    Tower,
}

pub struct Runway {
    pub designator: &'static str,
    pub heading: f64, // magnetic heading in degrees
}

pub struct AtcFacility {
    pub name: &'static str,
    pub callsign: &'static str,
    pub facility_type: FacilityType,
    pub frequency: f32,
    pub airport_ident: Option<&'static str>,
    pub position: (f64, f64), // (lat_deg, lon_deg)
    pub active_runways: Vec<Runway>,
}

impl AtcFacility {
    /// Short display name for radio log: "SFO TWR", "NorCal"
    pub fn display_short(&self) -> &'static str {
        match self.name {
            "NorCal Approach" => "NorCal",
            "SFO Tower" => "SFO TWR",
            "OAK Tower" => "OAK TWR",
            "SJC Tower" => "SJC TWR",
            "HWD Tower" => "HWD TWR",
            "PAO Tower" => "PAO TWR",
            "SQL Tower" => "SQL TWR",
            _ => self.name,
        }
    }
}

/// Build all Bay Area ATC facilities.
pub fn build_facilities() -> Vec<AtcFacility> {
    vec![
        AtcFacility {
            name: "NorCal Approach",
            callsign: "NorCal Approach",
            facility_type: FacilityType::Approach,
            frequency: 135.65,
            airport_ident: None,
            position: (37.7, -122.2), // approximate TRACON center
            active_runways: vec![],
        },
        AtcFacility {
            name: "SFO Tower",
            callsign: "San Francisco Tower",
            facility_type: FacilityType::Tower,
            frequency: 120.5,
            airport_ident: Some("KSFO"),
            position: (37.6213, -122.3790),
            active_runways: vec![
                Runway { designator: "28L", heading: 280.0 },
                Runway { designator: "28R", heading: 280.0 },
            ],
        },
        AtcFacility {
            name: "OAK Tower",
            callsign: "Oakland Tower",
            facility_type: FacilityType::Tower,
            frequency: 118.3,
            airport_ident: Some("KOAK"),
            position: (37.7213, -122.2208),
            active_runways: vec![
                Runway { designator: "30", heading: 300.0 },
            ],
        },
        AtcFacility {
            name: "SJC Tower",
            callsign: "San Jose Tower",
            facility_type: FacilityType::Tower,
            frequency: 124.0,
            airport_ident: Some("KSJC"),
            position: (37.3626, -121.9291),
            active_runways: vec![
                Runway { designator: "30L", heading: 300.0 },
            ],
        },
        AtcFacility {
            name: "HWD Tower",
            callsign: "Hayward Tower",
            facility_type: FacilityType::Tower,
            frequency: 120.2,
            airport_ident: Some("KHWD"),
            position: (37.6592, -122.1217),
            active_runways: vec![
                Runway { designator: "28L", heading: 280.0 },
            ],
        },
        AtcFacility {
            name: "PAO Tower",
            callsign: "Palo Alto Tower",
            facility_type: FacilityType::Tower,
            frequency: 118.6,
            airport_ident: Some("KPAO"),
            position: (37.4611, -122.1150),
            active_runways: vec![
                Runway { designator: "31", heading: 310.0 },
            ],
        },
        AtcFacility {
            name: "SQL Tower",
            callsign: "San Carlos Tower",
            facility_type: FacilityType::Tower,
            frequency: 119.2,
            airport_ident: Some("KSQL"),
            position: (37.5119, -122.2494),
            active_runways: vec![
                Runway { designator: "30", heading: 300.0 },
            ],
        },
    ]
}

/// Find facility index by frequency.
pub fn facility_by_freq(facilities: &[AtcFacility], freq: f32) -> Option<usize> {
    facilities.iter().position(|f| (f.frequency - freq).abs() < 0.01)
}

/// Find the NorCal Approach facility index.
pub fn norcal_index(facilities: &[AtcFacility]) -> usize {
    facilities.iter().position(|f| f.name == "NorCal Approach").unwrap_or(0)
}

/// Find the SFO Tower facility index.
pub fn sfo_index(facilities: &[AtcFacility]) -> usize {
    facilities.iter().position(|f| f.name == "SFO Tower").unwrap_or(1)
}
