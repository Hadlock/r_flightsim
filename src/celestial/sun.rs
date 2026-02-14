use glam::DVec3;

use super::{ecliptic_to_equatorial, obliquity_deg};
use super::time::jd_to_t;

pub struct SunResult {
    pub eci: DVec3,
    pub distance_m: f64,
}

/// Simplified solar position algorithm. Accurate to ~0.01 degrees.
pub fn sun_position(jd: f64) -> SunResult {
    let n = jd - 2_451_545.0;
    let t = n / 36_525.0;

    // Mean longitude (degrees)
    let l0 = (280.46646 + 36_000.76983 * t + 0.0003032 * t * t).rem_euclid(360.0);
    // Mean anomaly (degrees)
    let m = (357.52911 + 35_999.05029 * t - 0.0001537 * t * t).rem_euclid(360.0);
    let m_rad = m.to_radians();

    // Equation of center
    let c = 1.914_602 * m_rad.sin()
        + 0.019_993 * (2.0 * m_rad).sin()
        + 0.000_289 * (3.0 * m_rad).sin();

    // Sun's ecliptic longitude
    let sun_lon = (l0 + c).to_radians();
    let sun_lat = 0.0_f64;

    // Distance (AU)
    let e = 0.016_708_634 - 0.000_042_037 * t;
    let v = m_rad + c.to_radians();
    let r_au = 1.000_001_018 * (1.0 - e * e) / (1.0 + e * v.cos());
    let r_m = r_au * 149_597_870_700.0;

    // Convert to equatorial J2000
    let obliquity = obliquity_deg(jd_to_t(jd)).to_radians();
    let eci = ecliptic_to_equatorial(sun_lon, sun_lat, r_m, obliquity);

    SunResult { eci, distance_m: r_m }
}
