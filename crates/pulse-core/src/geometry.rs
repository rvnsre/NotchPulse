//! All input monitor coordinates are physical pixels; offsets are logical points.
//! Negative desktop origins and mixed DPI are intentionally preserved.
#[derive(Clone, Copy, Debug)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

pub fn anchor(m: MonitorRect, width: f64, height: f64, dx: f64, inset: f64) -> (i32, i32) {
    let scale = if m.scale.is_finite() && m.scale > 0.0 { m.scale } else { 1.0 };
    let w = (width.max(1.0) * scale).min(f64::from(m.width));
    let h = (height.max(1.0) * scale).min(f64::from(m.height));
    let left = f64::from(m.x);
    let top = f64::from(m.y);
    let x = (left + (f64::from(m.width) - w) / 2.0 + dx * scale)
        .clamp(left, left + f64::from(m.width) - w);
    let y = (top + inset * scale).clamp(top, top + f64::from(m.height) - h);
    (x.round() as i32, y.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retina_and_negative_origin() {
        let m = MonitorRect { x: -3024, y: -200, width: 3024, height: 1964, scale: 2.0 };
        assert_eq!(anchor(m, 460.0, 380.0, 0.0, 32.0), (-1972, -136));
    }
    #[test]
    fn clamps_camera_calibration_and_small_displays() {
        let m = MonitorRect { x: 100, y: 0, width: 320, height: 200, scale: 1.0 };
        assert_eq!(anchor(m, 460.0, 380.0, 4000.0, 400.0), (100, 0));
    }
}

