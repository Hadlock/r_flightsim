use std::time::SystemTime;

pub struct SimClock {
    /// Julian Date of the epoch (start time)
    epoch_jd: f64,
    /// Elapsed simulation seconds since epoch
    elapsed_sim: f64,
    /// Time warp factor (1.0 = real-time)
    pub time_scale: f64,
}

impl SimClock {
    /// Create a new SimClock. If `epoch_unix` is None, uses system clock.
    pub fn new(epoch_unix: Option<f64>) -> Self {
        let unix = epoch_unix.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs_f64()
        });
        Self {
            epoch_jd: unix_to_jd(unix),
            elapsed_sim: 0.0,
            time_scale: 1.0,
        }
    }

    /// Advance simulation clock by wall-clock dt (seconds).
    pub fn advance(&mut self, dt: f64) {
        self.elapsed_sim += dt * self.time_scale;
    }

    /// Current Julian Date.
    pub fn jd(&self) -> f64 {
        self.epoch_jd + self.elapsed_sim / 86_400.0
    }
}

/// Unix timestamp (seconds since 1970-01-01T00:00:00Z) to Julian Date.
pub fn unix_to_jd(unix_secs: f64) -> f64 {
    2_440_587.5 + unix_secs / 86_400.0
}

/// Julian centuries since J2000.0.
pub fn jd_to_t(jd: f64) -> f64 {
    (jd - 2_451_545.0) / 36_525.0
}

/// Greenwich Mean Sidereal Time in degrees from Julian Date.
pub fn gmst_deg(jd: f64) -> f64 {
    let t = jd_to_t(jd);
    let gmst = 280.46061837
        + 360.98564736629 * (jd - 2_451_545.0)
        + 0.000387933 * t * t
        - t * t * t / 38_710_000.0;
    gmst.rem_euclid(360.0)
}

/// Parse a subset of ISO 8601: "YYYY-MM-DDTHH:MM:SSZ" to Unix timestamp.
/// Returns Err if the format doesn't match.
pub fn iso8601_to_unix(s: &str) -> Result<f64, String> {
    let s = s.trim();
    if s.len() < 19 {
        return Err(format!("Too short: '{}'", s));
    }

    let parse = |slice: &str, name: &str| -> Result<i64, String> {
        slice
            .parse::<i64>()
            .map_err(|_| format!("Invalid {}: '{}'", name, slice))
    };

    let year = parse(&s[0..4], "year")?;
    let month = parse(&s[5..7], "month")?;
    let day = parse(&s[8..10], "day")?;
    let hour = parse(&s[11..13], "hour")?;
    let min = parse(&s[14..16], "minute")?;
    let sec = parse(&s[17..19], "second")?;

    if month < 1 || month > 12 || day < 1 || day > 31 {
        return Err(format!("Invalid date: {}-{}-{}", year, month, day));
    }

    // Convert to Unix timestamp using a simplified algorithm
    // Days from civil date to Unix epoch (1970-01-01)
    let y = if month <= 2 { year - 1 } else { year };
    let m = if month <= 2 { month + 9 } else { month - 3 };
    let days = 365 * y + y / 4 - y / 100 + y / 400 + (m * 306 + 5) / 10 + (day - 1) - 719468;

    Ok(days as f64 * 86400.0 + hour as f64 * 3600.0 + min as f64 * 60.0 + sec as f64)
}
