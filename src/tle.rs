use crate::aircraft_profile::OrbitSpec;

const CELESTRAK_URL: &str = "https://celestrak.org/NORAD/elements/gp.php";
const FETCH_TIMEOUT_SECS: u64 = 3;
const R_EARTH_KM: f64 = 6378.137;
const GM_EARTH: f64 = 398600.4418; // km^3/s^2

/// Parsed TLE data (line 1 + line 2 fields we care about).
#[derive(Debug, Clone)]
struct TleData {
    inclination_deg: f64,
    raan_deg: f64,
    eccentricity: f64,
    arg_periapsis_deg: f64,
    mean_anomaly_deg: f64,
    mean_motion: f64, // revolutions per day
}

/// Fetch TLE text from CelesTrak for a given NORAD catalog ID.
/// Returns the raw 3LE text (name + line1 + line2) or None on failure.
fn fetch_tle(norad_id: u32) -> Option<String> {
    let url = format!(
        "{}?CATNR={}&FORMAT=3LE",
        CELESTRAK_URL, norad_id
    );
    log::info!("[tle] fetching TLE for NORAD {} ...", norad_id);

    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS)))
        .build()
        .new_agent();

    let body = agent
        .get(&url)
        .call()
        .ok()?
        .into_body()
        .read_to_string()
        .ok()?;

    if body.trim().is_empty() || body.contains("No GP data found") {
        log::warn!("[tle] no TLE data for NORAD {}", norad_id);
        return None;
    }

    log::info!("[tle] received {} bytes", body.len());
    Some(body)
}

/// Parse a TLE (two-line element set) from text.
/// Accepts either 2LE (line1 + line2) or 3LE (name + line1 + line2).
fn parse_tle(text: &str) -> Option<TleData> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();

    // Find line 1 (starts with '1 ') and line 2 (starts with '2 ')
    let line1 = lines.iter().find(|l| l.starts_with("1 "))?;
    let line2 = lines.iter().find(|l| l.starts_with("2 "))?;

    // Validate minimum lengths
    if line1.len() < 68 || line2.len() < 68 {
        log::warn!("[tle] TLE lines too short: L1={} L2={}", line1.len(), line2.len());
        return None;
    }

    // Line 2 fixed-width columns (0-indexed):
    //  8-15: Inclination (degrees)
    // 17-24: RAAN (degrees)
    // 26-32: Eccentricity (leading decimal point implied)
    // 34-41: Argument of Perigee (degrees)
    // 43-50: Mean Anomaly (degrees)
    // 52-62: Mean Motion (revolutions per day)
    let inclination_deg: f64 = line2.get(8..16)?.trim().parse().ok()?;
    let raan_deg: f64 = line2.get(17..25)?.trim().parse().ok()?;
    let ecc_str = line2.get(26..33)?.trim();
    let eccentricity: f64 = format!("0.{}", ecc_str).parse().ok()?;
    let arg_periapsis_deg: f64 = line2.get(34..42)?.trim().parse().ok()?;
    let mean_anomaly_deg: f64 = line2.get(43..51)?.trim().parse().ok()?;
    let mean_motion: f64 = line2.get(52..63)?.trim().parse().ok()?;

    log::info!(
        "[tle] parsed: inc={:.2} raan={:.2} ecc={:.6} argpe={:.2} ma={:.2} mm={:.8}",
        inclination_deg, raan_deg, eccentricity,
        arg_periapsis_deg, mean_anomaly_deg, mean_motion
    );

    Some(TleData {
        inclination_deg,
        raan_deg,
        eccentricity,
        arg_periapsis_deg,
        mean_anomaly_deg,
        mean_motion,
    })
}

/// Solve Kepler's equation M = E - e*sin(E) for eccentric anomaly E.
fn solve_kepler(m_rad: f64, e: f64) -> f64 {
    let mut big_e = m_rad + e * m_rad.sin();
    for _ in 0..15 {
        let de = (big_e - e * big_e.sin() - m_rad) / (1.0 - e * big_e.cos());
        big_e -= de;
        if de.abs() < 1e-12 {
            break;
        }
    }
    big_e
}

/// Convert mean anomaly + eccentricity to true anomaly (radians).
fn mean_to_true_anomaly(mean_anomaly_rad: f64, e: f64) -> f64 {
    let big_e = solve_kepler(mean_anomaly_rad, e);
    // True anomaly from eccentric anomaly
    let sin_nu = (1.0 - e * e).sqrt() * big_e.sin() / (1.0 - e * big_e.cos());
    let cos_nu = (big_e.cos() - e) / (1.0 - e * big_e.cos());
    sin_nu.atan2(cos_nu)
}

/// Convert mean motion (rev/day) to semi-major axis (km).
fn mean_motion_to_sma_km(n: f64) -> f64 {
    // n in rev/day → rad/s
    let n_rad_s = n * 2.0 * std::f64::consts::PI / 86400.0;
    // a = (GM / n^2)^(1/3)
    (GM_EARTH / (n_rad_s * n_rad_s)).cbrt()
}

/// Apply TLE data to an OrbitSpec, overwriting orbital elements.
fn apply_tle(tle: &TleData, orbit: &mut OrbitSpec) {
    let sma_km = mean_motion_to_sma_km(tle.mean_motion);
    let e = tle.eccentricity;

    let perigee_km = sma_km * (1.0 - e) - R_EARTH_KM;
    let apogee_km = sma_km * (1.0 + e) - R_EARTH_KM;

    let true_anomaly_rad = mean_to_true_anomaly(
        tle.mean_anomaly_deg.to_radians(),
        e,
    );
    let true_anomaly_deg = true_anomaly_rad.to_degrees().rem_euclid(360.0);

    log::info!(
        "[tle] applied: alt={:.1} km apogee={:.1} km inc={:.2} raan={:.2} argpe={:.2} ta={:.2}",
        perigee_km, apogee_km, tle.inclination_deg, tle.raan_deg,
        tle.arg_periapsis_deg, true_anomaly_deg
    );

    orbit.altitude_km = perigee_km;
    orbit.inclination_deg = tle.inclination_deg;
    orbit.raan_deg = tle.raan_deg;
    orbit.arg_periapsis_deg = tle.arg_periapsis_deg;
    orbit.true_anomaly_deg = true_anomaly_deg;

    // Only set apogee if orbit is significantly non-circular
    if (apogee_km - perigee_km).abs() > 10.0 {
        orbit.apogee_km = Some(apogee_km);
    } else {
        orbit.apogee_km = None;
    }
}

/// Fetch live TLE from CelesTrak and apply it to the orbit spec.
/// Returns true if TLE was successfully fetched and applied, false otherwise.
/// On any failure, the orbit spec is left unchanged (uses profile defaults).
pub fn fetch_and_apply_tle(norad_id: u32, orbit: &mut OrbitSpec) -> bool {
    let text = match fetch_tle(norad_id) {
        Some(t) => t,
        None => {
            log::warn!("[tle] fetch failed for NORAD {}, using profile defaults", norad_id);
            return false;
        }
    };

    let tle = match parse_tle(&text) {
        Some(t) => t,
        None => {
            log::warn!("[tle] parse failed for NORAD {}, using profile defaults", norad_id);
            return false;
        }
    };

    apply_tle(&tle, orbit);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    // Sample ISS TLE (fixed example for reproducible tests)
    const ISS_TLE: &str = "\
ISS (ZARYA)
1 25544U 98067A   25045.18141127  .00021057  00000-0  37436-3 0  9991
2 25544  51.6388 294.8370 0002488  28.3578 331.7655 15.50110572495730";

    #[test]
    fn test_parse_tle() {
        let tle = parse_tle(ISS_TLE).expect("should parse ISS TLE");
        assert!((tle.inclination_deg - 51.6388).abs() < 0.001);
        assert!((tle.raan_deg - 294.837).abs() < 0.01);
        assert!((tle.eccentricity - 0.0002488).abs() < 1e-7);
        assert!((tle.arg_periapsis_deg - 28.3578).abs() < 0.001);
        assert!((tle.mean_anomaly_deg - 331.7655).abs() < 0.001);
        assert!((tle.mean_motion - 15.50110572).abs() < 0.001);
    }

    #[test]
    fn test_mean_motion_to_sma() {
        // ISS: ~15.5 rev/day → ~6780 km SMA (~408 km altitude)
        let sma = mean_motion_to_sma_km(15.5);
        assert!((sma - 6780.0).abs() < 20.0, "ISS SMA should be ~6780 km, got {:.1}", sma);
    }

    #[test]
    fn test_solve_kepler_circular() {
        // For e=0, E should equal M
        let m = 1.234;
        let e = solve_kepler(m, 0.0);
        assert!((e - m).abs() < 1e-10);
    }

    #[test]
    fn test_solve_kepler_elliptical() {
        // For e=0.5, M=1.0, verify Kepler's equation
        let m = 1.0;
        let e_val = 0.5;
        let big_e = solve_kepler(m, e_val);
        let residual = big_e - e_val * big_e.sin() - m;
        assert!(residual.abs() < 1e-10, "Kepler residual: {}", residual);
    }

    #[test]
    fn test_mean_to_true_anomaly_circular() {
        // For e~0, true anomaly ≈ mean anomaly
        let ma = 1.5;
        let ta = mean_to_true_anomaly(ma, 1e-9);
        assert!((ta - ma).abs() < 1e-6);
    }

    #[test]
    fn test_apply_tle_iss() {
        let tle = parse_tle(ISS_TLE).unwrap();
        let mut orbit = OrbitSpec {
            altitude_km: 420.0,
            apogee_km: None,
            inclination_deg: 51.6,
            raan_deg: 0.0,
            arg_periapsis_deg: 0.0,
            true_anomaly_deg: 0.0,
            camera_pitch_deg: -89.0,
            lagrange_point: None,
            fov_deg: None,
            norad_id: Some(25544),
        };
        apply_tle(&tle, &mut orbit);

        // Altitude should be ~415-425 km for ISS
        assert!(orbit.altitude_km > 400.0 && orbit.altitude_km < 440.0,
            "ISS altitude should be ~420 km, got {:.1}", orbit.altitude_km);
        // RAAN should match TLE
        assert!((orbit.raan_deg - 294.837).abs() < 0.01);
        // Inclination should match
        assert!((orbit.inclination_deg - 51.6388).abs() < 0.001);
        // Nearly circular → no apogee override
        assert!(orbit.apogee_km.is_none(), "ISS is nearly circular");
    }
}
