use eframe::egui;

/// Calculates the shortest distance from point `p` to segment `ab`.
pub fn dist_to_segment(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ap = p - a;
    let ab = b - a;
    let ab_len_sq = ab.length_sq();
    if ab_len_sq < 1e-6 {
        return p.distance(a);
    }
    let t = (ap.dot(ab) / ab_len_sq).clamp(0.0, 1.0);
    let projection = a + ab * t;
    p.distance(projection)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dist_to_segment_endpoints_and_midpoint() {
        let a = egui::pos2(0.0, 0.0);
        let b = egui::pos2(10.0, 0.0);

        // Point on the segment
        assert!((dist_to_segment(egui::pos2(5.0, 0.0), a, b) - 0.0).abs() < 1e-5);
        // Point perpendicular to midpoint
        assert!((dist_to_segment(egui::pos2(5.0, 3.0), a, b) - 3.0).abs() < 1e-5);
        // Point past endpoint a
        assert!((dist_to_segment(egui::pos2(-4.0, 3.0), a, b) - 5.0).abs() < 1e-5);
        // Degenerate segment (a == b)
        assert!((dist_to_segment(egui::pos2(3.0, 4.0), a, a) - 5.0).abs() < 1e-5);
    }
}
