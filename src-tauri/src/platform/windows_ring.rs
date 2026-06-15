//! Rasterizes the tray progress ring: a clockwise-from-12-o'clock arc on a
//! faint full circle, in `ink` (the caller picks a color that contrasts with
//! the current taskbar theme). Pure pixel math — no OS calls — so it's
//! unit-testable everywhere.

/// Returns `size`×`size` RGBA pixels for `fraction` (0..=1) progress, drawn in
/// `ink` (arc at full alpha, track faint).
pub fn render(fraction: f64, size: u32, ink: [u8; 3]) -> Vec<u8> {
    let fraction = fraction.clamp(0.0, 1.0);
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let center = (size as f64 - 1.0) / 2.0;
    let outer = size as f64 / 2.0 - 1.0;
    let inner = outer * 0.62;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f64 - center;
            let dy = y as f64 - center;
            let r = (dx * dx + dy * dy).sqrt();
            if r < inner || r > outer {
                continue;
            }
            // Angle clockwise from 12 o'clock, normalized to 0..1.
            let turn = (dx.atan2(-dy) / std::f64::consts::TAU).rem_euclid(1.0);
            let alpha: u8 = if turn <= fraction { 255 } else { 60 };
            let i = ((y * size + x) * 4) as usize;
            rgba[i..i + 3].copy_from_slice(&ink);
            rgba[i + 3] = alpha;
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::render;

    const SIZE: u32 = 32;
    const WHITE: [u8; 3] = [255, 255, 255];

    fn alpha_at(rgba: &[u8], x: u32, y: u32) -> u8 {
        rgba[((y * SIZE + x) * 4 + 3) as usize]
    }

    fn rgb_at(rgba: &[u8], x: u32, y: u32) -> [u8; 3] {
        let i = ((y * SIZE + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    }

    #[test]
    fn center_is_transparent_ring_is_opaque_when_done() {
        let px = render(1.0, SIZE, WHITE);
        assert_eq!(alpha_at(&px, 16, 16), 0, "center must stay empty");
        assert_eq!(alpha_at(&px, 16, 2), 255, "12 o'clock on the ring band");
    }

    #[test]
    fn zero_progress_leaves_only_the_faint_track() {
        let px = render(0.0, SIZE, WHITE);
        assert!(px.chunks(4).all(|p| p[3] == 0 || p[3] == 60));
    }

    #[test]
    fn half_progress_fills_right_side_only() {
        let px = render(0.5, SIZE, WHITE);
        assert_eq!(alpha_at(&px, 28, 16), 255, "3 o'clock filled");
        assert_eq!(alpha_at(&px, 3, 16), 60, "9 o'clock still faint");
    }

    #[test]
    fn out_of_range_fractions_clamp() {
        assert_eq!(render(-1.0, SIZE, WHITE), render(0.0, SIZE, WHITE));
        assert_eq!(render(2.0, SIZE, WHITE), render(1.0, SIZE, WHITE));
    }

    #[test]
    fn ring_pixels_use_the_requested_ink() {
        let ink = [0x26, 0x26, 0x2b];
        let px = render(1.0, SIZE, ink);
        // 12 o'clock sits on the ring band, so it carries the ink color.
        assert_eq!(rgb_at(&px, 16, 2), ink, "ring band must be drawn in ink");
    }
}
