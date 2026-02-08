use glam::DVec3;

// WGS-84 ellipsoid parameters
const WGS84_A: f64 = 6_378_137.0; // semi-major axis (m)
const WGS84_F: f64 = 1.0 / 298.257_223_563; // flattening
const WGS84_B: f64 = WGS84_A * (1.0 - WGS84_F); // semi-minor axis
const WGS84_E2: f64 = 1.0 - (WGS84_B * WGS84_B) / (WGS84_A * WGS84_A); // first eccentricity squared

/// Geodetic position: latitude (rad), longitude (rad), altitude above ellipsoid (m)
#[derive(Debug, Clone, Copy)]
pub struct LLA {
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
}

/// East-North-Up rotation frame at a given lat/lon.
/// Columns are the ENU axes expressed in ECEF.
#[derive(Debug, Clone, Copy)]
pub struct ENUFrame {
    pub east: DVec3,
    pub north: DVec3,
    pub up: DVec3,
    pub origin_ecef: DVec3,
}

/// Compute ENU basis vectors from geodetic lat/lon (radians)
fn enu_axes(lat: f64, lon: f64) -> (DVec3, DVec3, DVec3) {
    let (slat, clat) = lat.sin_cos();
    let (slon, clon) = lon.sin_cos();

    let east = DVec3::new(-slon, clon, 0.0);
    let north = DVec3::new(-slat * clon, -slat * slon, clat);
    let up = DVec3::new(clat * clon, clat * slon, slat);

    (east, north, up)
}

/// Geodetic (lat/lon/alt) to ECEF XYZ
pub fn lla_to_ecef(lla: &LLA) -> DVec3 {
    let (slat, clat) = lla.lat.sin_cos();
    let (slon, clon) = lla.lon.sin_cos();

    // Radius of curvature in the prime vertical
    let n = WGS84_A / (1.0 - WGS84_E2 * slat * slat).sqrt();

    let x = (n + lla.alt) * clat * clon;
    let y = (n + lla.alt) * clat * slon;
    let z = (n * (1.0 - WGS84_E2) + lla.alt) * slat;

    DVec3::new(x, y, z)
}

/// ECEF XYZ to geodetic using Bowring's iterative method (3 iterations)
pub fn ecef_to_lla(ecef: DVec3) -> LLA {
    let x = ecef.x;
    let y = ecef.y;
    let z = ecef.z;

    let p = (x * x + y * y).sqrt();
    let lon = y.atan2(x);

    // Initial estimate using Bowring's method
    let mut lat = z.atan2(p * (1.0 - WGS84_E2));

    // Iterate 3 times (converges very quickly)
    for _ in 0..3 {
        let slat = lat.sin();
        let n = WGS84_A / (1.0 - WGS84_E2 * slat * slat).sqrt();
        lat = (z + WGS84_E2 * n * slat).atan2(p);
    }

    let slat = lat.sin();
    let clat = lat.cos();
    let n = WGS84_A / (1.0 - WGS84_E2 * slat * slat).sqrt();

    // Altitude: avoid division by zero near poles
    let alt = if clat.abs() > 1e-10 {
        p / clat - n
    } else {
        z.abs() / slat.abs() - n * (1.0 - WGS84_E2)
    };

    LLA { lat, lon, alt }
}

/// Compute the ENU frame at a given lat/lon with ECEF origin
pub fn enu_frame_at(lat_rad: f64, lon_rad: f64, origin_ecef: DVec3) -> ENUFrame {
    let (east, north, up) = enu_axes(lat_rad, lon_rad);
    ENUFrame {
        east,
        north,
        up,
        origin_ecef,
    }
}

impl ENUFrame {
    /// Convert a vector from ENU to ECEF (rotation only, no translation)
    pub fn enu_to_ecef(&self, enu: DVec3) -> DVec3 {
        // ENU vector = e * east + n * north + u * up
        self.east * enu.x + self.north * enu.y + self.up * enu.z
    }

    /// Convert a vector from ECEF to ENU (rotation only, no translation)
    pub fn ecef_to_enu(&self, ecef: DVec3) -> DVec3 {
        // Project onto each ENU axis
        DVec3::new(
            ecef.dot(self.east),
            ecef.dot(self.north),
            ecef.dot(self.up),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOLERANCE_M: f64 = 0.001; // 1mm

    /// Helper: check round-trip lla → ecef → lla within tolerance
    fn assert_lla_roundtrip(lat_deg: f64, lon_deg: f64, alt: f64) {
        let original = LLA {
            lat: lat_deg.to_radians(),
            lon: lon_deg.to_radians(),
            alt,
        };
        let ecef = lla_to_ecef(&original);
        let result = ecef_to_lla(ecef);

        let original_ecef = lla_to_ecef(&original);
        let result_ecef = lla_to_ecef(&result);
        let error = (original_ecef - result_ecef).length();

        assert!(
            error < TOLERANCE_M,
            "Round-trip error {error:.6}m for ({lat_deg}, {lon_deg}, {alt})"
        );
    }

    #[test]
    fn roundtrip_equator_prime_meridian() {
        assert_lla_roundtrip(0.0, 0.0, 0.0);
    }

    #[test]
    fn roundtrip_equator_with_altitude() {
        assert_lla_roundtrip(0.0, 0.0, 10000.0);
    }

    #[test]
    fn roundtrip_north_pole() {
        assert_lla_roundtrip(90.0, 0.0, 0.0);
    }

    #[test]
    fn roundtrip_south_pole() {
        assert_lla_roundtrip(-90.0, 0.0, 0.0);
    }

    #[test]
    fn roundtrip_sfo() {
        assert_lla_roundtrip(37.613931, -122.358089, 0.0);
    }

    #[test]
    fn roundtrip_high_altitude() {
        assert_lla_roundtrip(45.0, 90.0, 35000.0);
    }

    #[test]
    fn roundtrip_southern_hemisphere() {
        assert_lla_roundtrip(-33.8688, 151.2093, 50.0); // Sydney
    }

    #[test]
    fn sfo_ecef_sanity() {
        // SFO airport: roughly (37.6°N, 122.4°W)
        // Expected ECEF: ~(-2.7M, -4.3M, 3.9M) meters
        let lla = LLA {
            lat: 37.613931_f64.to_radians(),
            lon: (-122.358089_f64).to_radians(),
            alt: 0.0,
        };
        let ecef = lla_to_ecef(&lla);

        // X should be negative (west of prime meridian, near 240° longitude in ECEF)
        // For SFO: approximately x ~ -2694893, y ~ -4297405, z ~ 3854586
        assert!(
            ecef.x.abs() > 2_000_000.0 && ecef.x.abs() < 3_000_000.0,
            "SFO ECEF X out of range: {}",
            ecef.x
        );
        assert!(
            ecef.y.abs() > 4_000_000.0 && ecef.y.abs() < 5_000_000.0,
            "SFO ECEF Y out of range: {}",
            ecef.y
        );
        assert!(
            ecef.z > 3_500_000.0 && ecef.z < 4_500_000.0,
            "SFO ECEF Z out of range: {}",
            ecef.z
        );
    }

    #[test]
    fn enu_axes_equator_prime_meridian() {
        // At (0°, 0°): east = (0, 1, 0), north = (0, 0, 1), up = (1, 0, 0)
        let (east, north, up) = enu_axes(0.0, 0.0);

        let tol = 1e-12;
        assert!((east - DVec3::new(0.0, 1.0, 0.0)).length() < tol, "east: {east:?}");
        assert!((north - DVec3::new(0.0, 0.0, 1.0)).length() < tol, "north: {north:?}");
        assert!((up - DVec3::new(1.0, 0.0, 0.0)).length() < tol, "up: {up:?}");
    }

    #[test]
    fn enu_axes_north_pole() {
        // At north pole (90°N, any lon): up should be (0, 0, 1) in ECEF
        let (_, _, up) = enu_axes(90.0_f64.to_radians(), 0.0);

        let tol = 1e-12;
        assert!(
            (up - DVec3::new(0.0, 0.0, 1.0)).length() < tol,
            "north pole up: {up:?}"
        );
    }

    #[test]
    fn enu_roundtrip_vector() {
        // A vector converted ENU→ECEF→ENU should remain the same
        let lat = 37.613931_f64.to_radians();
        let lon = (-122.358089_f64).to_radians();
        let origin = lla_to_ecef(&LLA { lat, lon, alt: 0.0 });
        let frame = enu_frame_at(lat, lon, origin);

        let enu_vec = DVec3::new(100.0, 200.0, 50.0);
        let ecef_vec = frame.enu_to_ecef(enu_vec);
        let back = frame.ecef_to_enu(ecef_vec);

        let tol = 1e-9;
        assert!(
            (enu_vec - back).length() < tol,
            "ENU roundtrip failed: {enu_vec:?} vs {back:?}"
        );
    }

    #[test]
    fn enu_axes_orthonormal() {
        // ENU axes should be orthonormal at any point
        let lat = 37.613931_f64.to_radians();
        let lon = (-122.358089_f64).to_radians();
        let (east, north, up) = enu_axes(lat, lon);

        let tol = 1e-12;
        assert!((east.dot(north)).abs() < tol, "east·north = {}", east.dot(north));
        assert!((east.dot(up)).abs() < tol, "east·up = {}", east.dot(up));
        assert!((north.dot(up)).abs() < tol, "north·up = {}", north.dot(up));
        assert!((east.length() - 1.0).abs() < tol);
        assert!((north.length() - 1.0).abs() < tol);
        assert!((up.length() - 1.0).abs() < tol);
    }
}
