//! Displays and the region the pet is allowed to roam in.
//!
//! All coordinates here are **logical global screen points** with the origin at
//! the top-left of the primary display (the same space AppKit uses after
//! flipping the Y axis).  On a mixed-DPI setup the logical space is the only
//! one that stays continuous across screens; physical pixels are derived per
//! display with [`Display::scale`] at the rendering boundary.

use serde::{Deserialize, Serialize};

use crate::geometry::{Point, Rect};

/// One connected display, as reported by the platform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Display {
    /// Stable platform identifier (`CGDirectDisplayID` on macOS).
    pub id: u32,
    /// Human-readable name, e.g. `NSScreen.localizedName`.
    pub name: String,
    /// Usable area (menu bar / Dock excluded), logical global points.
    pub bounds: Rect,
    /// Backing scale factor (2.0 on Retina, 1.0 on most external displays).
    pub scale: f32,
    /// The display that owns the menu bar and the origin of the global space.
    pub primary: bool,
}

impl Display {
    /// Logical size of the visible area, rounded for display in menus.
    pub fn size_label(&self) -> String {
        format!("{:.0}×{:.0}", self.bounds.w, self.bounds.h)
    }
}

/// Union of the display rectangles the pet may be in.
///
/// The union is intentionally a set of rectangles rather than one bounding box:
/// screens can be arranged with gaps (or only some of them may be enabled), and
/// the pet must never be clamped into an area that is not actually on a chosen
/// display.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RoamArea {
    rects: Vec<Rect>,
}

impl RoamArea {
    /// Builds an area, dropping degenerate/duplicate rectangles.
    pub fn new(rects: impl IntoIterator<Item = Rect>) -> Self {
        let mut kept: Vec<Rect> = Vec::new();
        for rect in rects {
            if rect.w <= 1.0 || rect.h <= 1.0 {
                continue;
            }
            if !kept.contains(&rect) {
                kept.push(rect);
            }
        }
        // Stable, spatial order keeps the tray/bounds deterministic.
        kept.sort_by(|a, b| {
            a.x.partial_cmp(&b.x)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
        });
        Self { rects: kept }
    }

    pub fn single(rect: Rect) -> Self {
        Self::new([rect])
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    pub fn rects(&self) -> &[Rect] {
        &self.rects
    }

    pub fn contains(&self, point: Point) -> bool {
        self.rects.iter().any(|rect| rect.contains(point))
    }

    /// Keeps `point` inside the nearest enabled rectangle.
    ///
    /// A point that is already on an enabled display is returned unchanged;
    /// otherwise it is projected onto the closest edge/corner.
    pub fn clamp(&self, point: Point) -> Point {
        if self.contains(point) {
            return point;
        }
        self.nearest_point(point).unwrap_or(point)
    }

    /// The rectangle containing `point`, if any.
    pub fn rect_at(&self, point: Point) -> Option<Rect> {
        self.rects.iter().copied().find(|rect| rect.contains(point))
    }

    /// The rectangle whose clamped point is closest to `point`.
    pub fn nearest_rect(&self, point: Point) -> Option<Rect> {
        self.rects.iter().copied().min_by(|a, b| {
            distance_sq(point, project(*a, point))
                .partial_cmp(&distance_sq(point, project(*b, point)))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Whether the straight segment from `from` to `to` stays on an enabled
    /// display (sampled).  Used before starting a walk so two screens with a
    /// gap between them are never crossed through the desktop void.
    pub fn segment_inside(&self, from: Point, to: Point) -> bool {
        const STEPS: u32 = 32;
        (0..=STEPS).all(|step| {
            let t = step as f32 / STEPS as f32;
            self.contains(Point::new(
                from.x + (to.x - from.x) * t,
                from.y + (to.y - from.y) * t,
            ))
        })
    }

    pub fn nearest_point(&self, point: Point) -> Option<Point> {
        self.rects
            .iter()
            .map(|rect| project(*rect, point))
            .min_by(|a, b| {
                distance_sq(point, *a)
                    .partial_cmp(&distance_sq(point, *b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Smallest-to-largest bounding box of every rectangle (used only for
    /// fallbacks such as window clipping when no display is known).
    pub fn union(&self) -> Option<Rect> {
        let first = *self.rects.first()?;
        let (mut x0, mut y0) = (first.x, first.y);
        let (mut x1, mut y1) = (first.right(), first.bottom());
        for rect in &self.rects[1..] {
            x0 = x0.min(rect.x);
            y0 = y0.min(rect.y);
            x1 = x1.max(rect.right());
            y1 = y1.max(rect.bottom());
        }
        Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
    }
}

fn project(rect: Rect, point: Point) -> Point {
    Point::new(
        point.x.clamp(rect.x, rect.right()),
        point.y.clamp(rect.y, rect.bottom()),
    )
}

fn distance_sq(a: Point, b: Point) -> f32 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_clamps_inside_points_unchanged() {
        let area = RoamArea::single(Rect::new(0.0, 0.0, 100.0, 100.0));
        let point = Point::new(50.0, 50.0);
        assert!(area.contains(point));
        assert_eq!(area.clamp(point), point);
        assert_eq!(area.rect_at(point), Some(Rect::new(0.0, 0.0, 100.0, 100.0)));
    }

    #[test]
    fn area_clamps_to_the_nearest_enabled_display() {
        // Two screens side by side with a 20 pt gap between them.
        let left = Rect::new(0.0, 0.0, 100.0, 80.0);
        let right = Rect::new(120.0, 0.0, 100.0, 80.0);
        let area = RoamArea::new([right, left]);
        assert_eq!(area.rects().len(), 2);
        assert_eq!(area.rects()[0].x, 0.0, "rectangles are sorted spatially");

        assert_eq!(area.clamp(Point::new(115.0, 40.0)), Point::new(120.0, 40.0));
        assert_eq!(area.clamp(Point::new(80.0, 40.0)), Point::new(80.0, 40.0));
        assert_eq!(area.clamp(Point::new(-30.0, 40.0)), Point::new(0.0, 40.0));
        assert_eq!(
            area.clamp(Point::new(115.0, 200.0)),
            Point::new(120.0, 80.0)
        );
        assert_eq!(area.nearest_rect(Point::new(115.0, 40.0)), Some(right));
    }

    #[test]
    fn empty_area_is_inert() {
        let area = RoamArea::default();
        assert!(area.is_empty());
        assert!(!area.contains(Point::new(1.0, 1.0)));
        assert_eq!(area.clamp(Point::new(12.0, 34.0)), Point::new(12.0, 34.0));
        assert_eq!(area.union(), None);
    }

    #[test]
    fn degenerate_rectangles_are_dropped() {
        let area = RoamArea::new([
            Rect::new(0.0, 0.0, 0.0, 100.0),
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(0.0, 0.0, 100.0, 100.0),
        ]);
        assert_eq!(area.rects().len(), 1);
    }

    #[test]
    fn segments_cross_shared_edges_but_not_gaps() {
        // Adjacent screens: a straight walk can cross the seam.
        let adjacent = RoamArea::new([
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(100.0, 0.0, 100.0, 100.0),
        ]);
        assert!(adjacent.segment_inside(Point::new(90.0, 50.0), Point::new(110.0, 50.0)));

        // Screens with a gap: the same walk must be rejected.
        let gap = RoamArea::new([
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(120.0, 0.0, 100.0, 100.0),
        ]);
        assert!(!gap.segment_inside(Point::new(90.0, 50.0), Point::new(130.0, 50.0)));
    }
}
