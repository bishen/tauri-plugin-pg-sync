use serde::{Deserialize, Serialize};
use geo::{Point, Rect, HaversineDistance};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GeoPoint {
    pub lng: f64,
    pub lat: f64,
}

impl GeoPoint {
    pub fn new(lng: f64, lat: f64) -> Self {
        Self { lng, lat }
    }

    pub fn to_geo_point(&self) -> Point<f64> {
        Point::new(self.lng, self.lat)
    }

    pub fn distance_to(&self, other: &GeoPoint) -> f64 {
        let p1 = self.to_geo_point();
        let p2 = other.to_geo_point();
        p1.haversine_distance(&p2)
    }

    pub fn to_wkt(&self) -> String {
        format!("POINT({} {})", self.lng, self.lat)
    }

    pub fn to_wkb(&self) -> Vec<u8> {
        let mut wkb = Vec::with_capacity(21);
        wkb.push(1);
        wkb.extend_from_slice(&1u32.to_le_bytes());
        wkb.extend_from_slice(&self.lng.to_le_bytes());
        wkb.extend_from_slice(&self.lat.to_le_bytes());
        wkb
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GeoBounds {
    pub min_lng: f64,
    pub min_lat: f64,
    pub max_lng: f64,
    pub max_lat: f64,
}

impl GeoBounds {
    pub fn new(min_lng: f64, min_lat: f64, max_lng: f64, max_lat: f64) -> Self {
        Self {
            min_lng,
            min_lat,
            max_lng,
            max_lat,
        }
    }

    pub fn contains(&self, point: &GeoPoint) -> bool {
        point.lng >= self.min_lng
            && point.lng <= self.max_lng
            && point.lat >= self.min_lat
            && point.lat <= self.max_lat
    }

    pub fn to_rect(&self) -> Rect<f64> {
        Rect::new(
            (self.min_lng, self.min_lat),
            (self.max_lng, self.max_lat),
        )
    }

    pub fn to_sqlite_mbr_params(&self) -> (f64, f64, f64, f64) {
        (self.min_lng, self.min_lat, self.max_lng, self.max_lat)
    }

    pub fn to_postgis_envelope(&self, srid: i32) -> String {
        format!(
            "ST_MakeEnvelope({}, {}, {}, {}, {})",
            self.min_lng, self.min_lat, self.max_lng, self.max_lat, srid
        )
    }
}

pub struct SpatialQuery;

impl SpatialQuery {
    pub fn sqlite_bbox_query(table: &str, geom_column: &str) -> String {
        format!(
            "SELECT * FROM {} WHERE MbrContains(BuildMbr(?1, ?2, ?3, ?4), {})",
            table, geom_column
        )
    }

    pub fn sqlite_distance_query(table: &str, geom_column: &str, limit: usize) -> String {
        format!(
            "SELECT *, Distance({}, MakePoint(?1, ?2)) as _distance FROM {} ORDER BY _distance LIMIT {}",
            geom_column, table, limit
        )
    }

    pub fn postgis_bbox_query(table: &str, geom_column: &str, srid: i32) -> String {
        format!(
            "SELECT * FROM {} WHERE ST_Contains(ST_MakeEnvelope($1, $2, $3, $4, {}), {})",
            table, srid, geom_column
        )
    }

    pub fn postgis_distance_query(table: &str, geom_column: &str, srid: i32, limit: usize) -> String {
        format!(
            "SELECT *, ST_Distance({}::geography, ST_SetSRID(ST_MakePoint($1, $2), {})::geography) as _distance FROM {} ORDER BY _distance LIMIT {}",
            geom_column, srid, table, limit
        )
    }

    pub fn postgis_within_radius_query(table: &str, geom_column: &str, srid: i32) -> String {
        format!(
            "SELECT * FROM {} WHERE ST_DWithin({}::geography, ST_SetSRID(ST_MakePoint($1, $2), {})::geography, $3)",
            table, geom_column, srid
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geo_point_distance() {
        let beijing = GeoPoint::new(116.4074, 39.9042);
        let shanghai = GeoPoint::new(121.4737, 31.2304);
        
        let distance = beijing.distance_to(&shanghai);
        assert!(distance > 1000000.0 && distance < 1100000.0);
    }

    #[test]
    fn test_bounds_contains() {
        let bounds = GeoBounds::new(116.0, 39.0, 117.0, 40.0);
        let inside = GeoPoint::new(116.5, 39.5);
        let outside = GeoPoint::new(118.0, 39.5);
        
        assert!(bounds.contains(&inside));
        assert!(!bounds.contains(&outside));
    }
}
