//! Geometry helpers shared by the state machine, window sizing and hit tests.
//!
//! All points are in **physical screen pixels** unless a name says otherwise.
//! The `pet_anchor` stored in the asset manifests is in *canvas units*; the
//! window rect is derived by scaling it by `scale * dpi` and subtracting it from
//! the screen-space anchor (the spot on screen where the pet's feet are).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Point) -> f32 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        (dx * dx + dy * dy).sqrt()
    }

    pub fn clamp(self, bounds: Rect) -> Point {
        Point::new(
            self.x.clamp(bounds.x, bounds.right()),
            self.y.clamp(bounds.y, bounds.bottom()),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn center(&self) -> Point {
        Point::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    /// Intersection of two rectangles, clamped to at least 1x1.
    pub fn intersect(&self, other: Rect) -> Rect {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = self.right().min(other.right());
        let y1 = self.bottom().min(other.bottom());
        Rect::new(x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0))
    }

    /// Shrink by `margin` on every side, never producing a negative rect.
    pub fn inset(&self, margin: f32) -> Rect {
        let w = (self.w - margin * 2.0).max(1.0);
        let h = (self.h - margin * 2.0).max(1.0);
        Rect::new(self.x + margin, self.y + margin, w, h)
    }

    /// Grow by `margin` on every side (used for the motion-trail stage).
    pub fn outset(&self, margin: f32) -> Rect {
        Rect::new(
            self.x - margin,
            self.y - margin,
            self.w + margin * 2.0,
            self.h + margin * 2.0,
        )
    }
}

/// The eight walk directions, named the way the asset manifests name them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Down,
    LeftDown,
    Left,
    LeftUp,
    Up,
    RightUp,
    Right,
    RightDown,
}

impl Direction {
    pub const ALL: [Direction; 8] = [
        Direction::Down,
        Direction::LeftDown,
        Direction::Left,
        Direction::LeftUp,
        Direction::Up,
        Direction::RightUp,
        Direction::Right,
        Direction::RightDown,
    ];

    pub fn file_key(self) -> &'static str {
        match self {
            Direction::Down => "down",
            Direction::LeftDown => "leftdown",
            Direction::Left => "left",
            Direction::LeftUp => "leftup",
            Direction::Up => "up",
            Direction::RightUp => "rightup",
            Direction::Right => "right",
            Direction::RightDown => "rightdown",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.file_key() == key)
    }

    /// Quantise a screen-space delta (y grows downwards) to one of eight
    /// directions.  The zero vector maps to [`Direction::Down`].
    pub fn from_delta(dx: f32, dy: f32) -> Self {
        if dx == 0.0 && dy == 0.0 {
            return Direction::Down;
        }
        let angle = dy.atan2(dx); // -pi..=pi, 0 = right, +pi/2 = down
        let sector = (angle / (std::f32::consts::PI / 4.0)).round() as i32;
        match sector.rem_euclid(8) {
            0 => Direction::Right,
            1 => Direction::RightDown,
            2 => Direction::Down,
            3 => Direction::LeftDown,
            4 => Direction::Left,
            5 => Direction::LeftUp,
            6 => Direction::Up,
            _ => Direction::RightUp,
        }
    }

    pub fn is_left(self) -> bool {
        matches!(
            self,
            Direction::Left | Direction::LeftDown | Direction::LeftUp
        )
    }
}

/// Window rect that puts the pet's anchor (feet) at `anchor` on screen.
///
/// `scale` is the user-facing zoom, `dpi` the monitor scale factor; both are
/// multiplied so the vector art is rasterised at physical resolution.
pub fn pet_window_rect(
    anchor: Point,
    canvas: (f32, f32),
    pet_anchor: (f32, f32),
    scale: f32,
    dpi: f32,
) -> Rect {
    let s = (scale * dpi).max(0.01);
    Rect::new(
        anchor.x - pet_anchor.0 * s,
        anchor.y - pet_anchor.1 * s,
        (canvas.0 * s).round().max(1.0),
        (canvas.1 * s).round().max(1.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_are_quantised_by_sector() {
        assert_eq!(Direction::from_delta(10.0, 0.0), Direction::Right);
        assert_eq!(Direction::from_delta(-10.0, 0.0), Direction::Left);
        assert_eq!(Direction::from_delta(0.0, 10.0), Direction::Down);
        assert_eq!(Direction::from_delta(0.0, -10.0), Direction::Up);
        assert_eq!(Direction::from_delta(10.0, 10.0), Direction::RightDown);
        assert_eq!(Direction::from_delta(-10.0, 10.0), Direction::LeftDown);
        assert_eq!(Direction::from_delta(-10.0, -10.0), Direction::LeftUp);
        assert_eq!(Direction::from_delta(10.0, -10.0), Direction::RightUp);
        assert_eq!(Direction::from_delta(0.0, 0.0), Direction::Down);
    }

    #[test]
    fn anchor_maps_to_window_origin() {
        // Walk canvas 66x96, anchor (33, 90), scale 1.5, dpi 2 => s = 3.
        let r = pet_window_rect(Point::new(1000.0, 800.0), (66.0, 96.0), (33.0, 90.0), 1.5, 2.0);
        assert_eq!(r.x, 1000.0 - 99.0);
        assert_eq!(r.y, 800.0 - 270.0);
        assert_eq!(r.w, 198.0);
        assert_eq!(r.h, 288.0);
    }

    #[test]
    fn window_rect_is_clipped_to_the_visible_screen() {
        let desired = Rect::new(-40.0, -30.0, 300.0, 400.0);
        let screen = Rect::new(0.0, 50.0, 5120.0, 2672.0);
        let visible = desired.intersect(screen);
        assert_eq!(visible, Rect::new(0.0, 50.0, 260.0, 320.0));
        // Fully inside: unchanged.
        let inside = Rect::new(10.0, 100.0, 50.0, 60.0);
        assert_eq!(inside.intersect(screen), inside);
    }

    #[test]
    fn outset_adds_equal_margins_for_the_trail_stage() {
        let tight = Rect::new(100.0, 200.0, 198.0, 288.0);
        assert_eq!(tight.outset(0.0), tight);
        assert_eq!(tight.outset(20.0), Rect::new(80.0, 180.0, 238.0, 328.0));
    }

    #[test]
    fn direction_keys_round_trip() {
        for dir in Direction::ALL {
            assert_eq!(Direction::from_key(dir.file_key()), Some(dir));
        }
    }
}
