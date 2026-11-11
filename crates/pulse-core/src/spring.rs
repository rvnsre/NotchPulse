//! Closed-form damped spring. Stable across frame drops; stops at rest.
#[derive(Clone, Copy, Debug, Default)]
pub struct Spring {
    pub value: f64,
    pub velocity: f64,
    pub target: f64,
}

impl Spring {
    pub fn advance(&mut self, dt: f64) -> bool {
        let dt = dt.clamp(0.0, 0.05);
        let omega: f64 = 22.0;
        let zeta: f64 = 0.82;
        let damp = omega * zeta;
        let wd = omega * (1.0 - zeta * zeta).sqrt();
        let x = self.value - self.target;
        let b = (self.velocity + damp * x) / wd;
        let (sin, cos) = (wd * dt).sin_cos();
        let decay = (-damp * dt).exp();
        self.value = self.target + decay * (x * cos + b * sin);
        self.velocity = decay * ((b * wd - damp * x) * cos - (x * wd + damp * b) * sin);
        let moving = (self.value - self.target).abs() > 0.0005 || self.velocity.abs() > 0.005;
        if !moving { self.value = self.target; self.velocity = 0.0; }
        moving
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settles_and_reverses_without_jumps() {
        let mut s = Spring { target: 1.0, ..Default::default() };
        for _ in 0..300 { s.advance(1.0/120.0); }
        assert_eq!(s.value, 1.0);
        s.target = 0.0;
        assert!(s.advance(1.0/60.0));
        assert!(s.value > 0.9);
        for _ in 0..300 { s.advance(1.0/60.0); }
        assert_eq!(s.value, 0.0);
    }
}

