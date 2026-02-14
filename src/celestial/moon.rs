use glam::DVec3;

use super::{ecliptic_to_equatorial, obliquity_deg};
use super::time::jd_to_t;

pub struct MoonResult {
    pub eci: DVec3,
    pub distance_m: f64,
    pub diameter_m: f64,
}

/// Truncated ELP2000 lunar position. Accuracy ~0.1 degrees (~700 km).
pub fn moon_position(jd: f64) -> MoonResult {
    let t = jd_to_t(jd);

    // Fundamental arguments (degrees)
    let lp = (218.3164477 + 481267.88123421 * t - 0.0015786 * t * t).rem_euclid(360.0);
    let d = (297.8501921 + 445267.1114034 * t - 0.0018819 * t * t).rem_euclid(360.0);
    let m = (357.5291092 + 35999.0502909 * t - 0.0001536 * t * t).rem_euclid(360.0);
    let mp = (134.9633964 + 477198.8675055 * t + 0.0087414 * t * t).rem_euclid(360.0);
    let f = (93.2720950 + 483202.0175233 * t - 0.0036539 * t * t).rem_euclid(360.0);

    let d_r = d.to_radians();
    let m_r = m.to_radians();
    let mp_r = mp.to_radians();
    let f_r = f.to_radians();

    // Longitude terms (top 24 terms from ELP2000)
    let sum_l = 6_288_774.0 * mp_r.sin()
        + 1_274_027.0 * (2.0 * d_r - mp_r).sin()
        + 658_314.0 * (2.0 * d_r).sin()
        + 213_618.0 * (2.0 * mp_r).sin()
        - 185_116.0 * m_r.sin()
        - 114_332.0 * (2.0 * f_r).sin()
        + 58_793.0 * (2.0 * d_r - 2.0 * mp_r).sin()
        + 57_066.0 * (2.0 * d_r - m_r - mp_r).sin()
        + 53_322.0 * (2.0 * d_r + mp_r).sin()
        + 45_758.0 * (2.0 * d_r - m_r).sin()
        - 40_923.0 * (m_r - mp_r).sin()
        - 34_720.0 * d_r.sin()
        - 30_383.0 * (m_r + mp_r).sin()
        + 15_327.0 * (2.0 * d_r - 2.0 * f_r).sin()
        - 12_528.0 * (mp_r + 2.0 * f_r).sin()
        + 10_980.0 * (mp_r - 2.0 * f_r).sin()
        + 10_675.0 * (4.0 * d_r - mp_r).sin()
        + 10_034.0 * (3.0 * mp_r).sin()
        + 8_548.0 * (4.0 * d_r - 2.0 * mp_r).sin()
        - 7_888.0 * (2.0 * d_r + m_r - mp_r).sin()
        - 6_766.0 * (2.0 * d_r + m_r).sin()
        - 5_163.0 * (d_r - mp_r).sin()
        + 4_987.0 * (d_r + m_r).sin()
        + 4_036.0 * (2.0 * d_r - m_r + mp_r).sin();

    // Latitude terms (top 10)
    let sum_b = 5_128_122.0 * f_r.sin()
        + 280_602.0 * (mp_r + f_r).sin()
        + 277_693.0 * (mp_r - f_r).sin()
        + 173_237.0 * (2.0 * d_r - f_r).sin()
        + 55_413.0 * (2.0 * d_r - mp_r + f_r).sin()
        + 46_271.0 * (2.0 * d_r - mp_r - f_r).sin()
        + 32_573.0 * (2.0 * d_r + f_r).sin()
        + 17_198.0 * (2.0 * mp_r + f_r).sin()
        + 9_266.0 * (2.0 * d_r + mp_r - f_r).sin()
        + 8_822.0 * (2.0 * mp_r - f_r).sin();

    // Distance terms (km corrections to mean distance)
    let sum_r = -20_905_355.0 * mp_r.cos()
        - 3_699_111.0 * (2.0 * d_r - mp_r).cos()
        - 2_955_968.0 * (2.0 * d_r).cos()
        - 569_925.0 * (2.0 * mp_r).cos()
        + 48_888.0 * m_r.cos()
        - 3_149.0 * (2.0 * f_r).cos()
        + 246_158.0 * (2.0 * d_r - 2.0 * mp_r).cos()
        - 152_138.0 * (2.0 * d_r - m_r - mp_r).cos()
        - 170_733.0 * (2.0 * d_r + mp_r).cos()
        - 204_586.0 * (2.0 * d_r - m_r).cos()
        - 129_620.0 * (m_r - mp_r).cos()
        + 108_743.0 * d_r.cos();

    // Ecliptic longitude, latitude, distance
    let lon_deg = lp + sum_l / 1_000_000.0;
    let lat_deg = sum_b / 1_000_000.0;
    let dist_km = 385_000.56 + sum_r / 1_000.0;
    let dist_m = dist_km * 1_000.0;

    // Convert to equatorial J2000
    let obliquity = obliquity_deg(t).to_radians();
    let eci = ecliptic_to_equatorial(lon_deg.to_radians(), lat_deg.to_radians(), dist_m, obliquity);

    MoonResult {
        eci,
        distance_m: dist_m,
        diameter_m: 3_474_800.0,
    }
}
