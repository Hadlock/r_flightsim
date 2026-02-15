use glam::DVec3;

use super::obliquity_deg;

/// Keplerian orbital elements at J2000.0 + rates per Julian century.
/// Source: JPL Solar System Dynamics, Standish (1992).
pub struct PlanetElements {
    pub name: &'static str,
    pub a: f64,         // semi-major axis (AU)
    pub e: f64,         // eccentricity
    pub i: f64,         // inclination (degrees)
    pub omega: f64,     // longitude of ascending node (degrees)
    pub w_bar: f64,     // longitude of perihelion (degrees)
    pub l: f64,         // mean longitude (degrees)
    pub a_dot: f64,
    pub e_dot: f64,
    pub i_dot: f64,
    pub omega_dot: f64,
    pub w_bar_dot: f64,
    pub l_dot: f64,
}

pub const PLANETS: [PlanetElements; 8] = [
    PlanetElements {
        name: "Mercury",
        a: 0.38709927, e: 0.20563593, i: 7.00497902,
        omega: 48.33076593, w_bar: 77.45779628, l: 252.25032350,
        a_dot: 0.00000037, e_dot: 0.00001906, i_dot: -0.00594749,
        omega_dot: -0.12534081, w_bar_dot: 0.16047689, l_dot: 149472.67411175,
    },
    PlanetElements {
        name: "Venus",
        a: 0.72333566, e: 0.00677672, i: 3.39467605,
        omega: 76.67984255, w_bar: 131.60246718, l: 181.97909950,
        a_dot: 0.00000390, e_dot: -0.00004107, i_dot: -0.00078890,
        omega_dot: -0.27769418, w_bar_dot: 0.00268329, l_dot: 58517.81538729,
    },
    PlanetElements {
        name: "Earth",
        a: 1.00000261, e: 0.01671123, i: -0.00001531,
        omega: 0.0, w_bar: 102.93768193, l: 100.46457166,
        a_dot: 0.00000562, e_dot: -0.00004392, i_dot: -0.01294668,
        omega_dot: 0.0, w_bar_dot: 0.32327364, l_dot: 35999.37244981,
    },
    PlanetElements {
        name: "Mars",
        a: 1.52371034, e: 0.09339410, i: 1.84969142,
        omega: 49.55953891, w_bar: -23.94362959, l: -4.55343205,
        a_dot: 0.00001847, e_dot: 0.00007882, i_dot: -0.00813131,
        omega_dot: -0.29257343, w_bar_dot: 0.44441088, l_dot: 19140.30268499,
    },
    PlanetElements {
        name: "Jupiter",
        a: 5.20288700, e: 0.04838624, i: 1.30439695,
        omega: 100.47390909, w_bar: 14.72847983, l: 34.39644051,
        a_dot: -0.00011607, e_dot: -0.00013253, i_dot: -0.00183714,
        omega_dot: 0.20469106, w_bar_dot: 0.21252668, l_dot: 3034.74612775,
    },
    PlanetElements {
        name: "Saturn",
        a: 9.53667594, e: 0.05386179, i: 2.48599187,
        omega: 113.66242448, w_bar: 92.59887831, l: 49.95424423,
        a_dot: -0.00125060, e_dot: -0.00050991, i_dot: 0.00193609,
        omega_dot: -0.28867794, w_bar_dot: -0.41897216, l_dot: 1222.49362201,
    },
    PlanetElements {
        name: "Uranus",
        a: 19.18916464, e: 0.04725744, i: 0.77263783,
        omega: 74.01692503, w_bar: 170.95427630, l: 313.23810451,
        a_dot: -0.00196176, e_dot: -0.00004397, i_dot: -0.00242939,
        omega_dot: 0.04240589, w_bar_dot: 0.40805281, l_dot: 428.48202785,
    },
    PlanetElements {
        name: "Neptune",
        a: 30.06992276, e: 0.00859048, i: 1.77004347,
        omega: 131.78422574, w_bar: 44.96476227, l: -55.12002969,
        a_dot: 0.00026291, e_dot: 0.00005105, i_dot: 0.00035372,
        omega_dot: -0.00508664, w_bar_dot: -0.32241464, l_dot: 218.45945325,
    },
];


/// Solve Kepler's equation M = E - e*sin(E) for E.
fn solve_kepler(m_rad: f64, e: f64) -> f64 {
    let mut big_e = m_rad + e * m_rad.sin(); // initial guess
    for _ in 0..10 {
        let de = (big_e - e * big_e.sin() - m_rad) / (1.0 - e * big_e.cos());
        big_e -= de;
        if de.abs() < 1e-12 {
            break;
        }
    }
    big_e
}

/// Compute heliocentric ecliptic position of a planet at Julian century T.
/// Returns position in meters in J2000 ecliptic frame.
fn heliocentric_position(el: &PlanetElements, t: f64) -> DVec3 {
    let a = el.a + el.a_dot * t;
    let e = el.e + el.e_dot * t;
    let i = (el.i + el.i_dot * t).to_radians();
    let omega = (el.omega + el.omega_dot * t).to_radians();
    let w_bar = (el.w_bar + el.w_bar_dot * t).to_radians();
    let l = (el.l + el.l_dot * t).to_radians();

    // Argument of perihelion
    let w = w_bar - omega;
    // Mean anomaly
    let m = (l - w_bar).rem_euclid(std::f64::consts::TAU);
    // Eccentric anomaly
    let big_e = solve_kepler(m, e);
    // True anomaly
    let v = 2.0 * ((1.0 + e).sqrt() * (big_e / 2.0).sin())
        .atan2((1.0 - e).sqrt() * (big_e / 2.0).cos());
    // Radius
    let r = a * (1.0 - e * big_e.cos());

    // Heliocentric ecliptic coordinates
    let cos_v_w = (v + w).cos();
    let sin_v_w = (v + w).sin();
    let cos_omega = omega.cos();
    let sin_omega = omega.sin();
    let cos_i = i.cos();
    let sin_i = i.sin();

    let x = r * (cos_omega * cos_v_w - sin_omega * sin_v_w * cos_i);
    let y = r * (sin_omega * cos_v_w + cos_omega * sin_v_w * cos_i);
    let z = r * (sin_v_w * sin_i);

    DVec3::new(x * crate::constants::AU_TO_M, y * crate::constants::AU_TO_M, z * crate::constants::AU_TO_M)
}

/// Compute geocentric ECI positions for the 7 non-Earth planets.
/// Returns [Mercury, Venus, Mars, Jupiter, Saturn, Uranus, Neptune].
pub fn compute_geocentric_positions(t: f64) -> [DVec3; 7] {
    let obliquity = obliquity_deg(t).to_radians();

    // Earth's heliocentric ecliptic position
    let earth_helio = heliocentric_position(&PLANETS[2], t);

    let mut result = [DVec3::ZERO; 7];
    let mut out_idx = 0;
    for (i, el) in PLANETS.iter().enumerate() {
        if i == 2 {
            continue; // skip Earth
        }
        let helio = heliocentric_position(el, t);
        let geo_ecliptic = helio - earth_helio;

        // Ecliptic Cartesian to equatorial J2000 (rotate around X by obliquity)
        let cos_e = obliquity.cos();
        let sin_e = obliquity.sin();
        result[out_idx] = DVec3::new(
            geo_ecliptic.x,
            geo_ecliptic.y * cos_e - geo_ecliptic.z * sin_e,
            geo_ecliptic.y * sin_e + geo_ecliptic.z * cos_e,
        );
        out_idx += 1;
    }
    result
}

/// Names in output order (Earth excluded).
pub const PLANET_NAMES: [&str; 7] = [
    "Mercury", "Venus", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
];
