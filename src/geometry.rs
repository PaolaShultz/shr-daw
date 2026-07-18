use ratatui::layout::Rect;

pub fn contains(r: Rect, x: u16, y: u16) -> bool {
    r.width > 0
        && r.height > 0
        && x >= r.x
        && x.saturating_sub(r.x) < r.width
        && y >= r.y
        && y.saturating_sub(r.y) < r.height
}

pub fn rect(x: u16, y: u16, width: u16, height: u16) -> Rect {
    Rect::new(x, y, width, height)
}

pub fn visible_index(list: Rect, offset: usize, x: u16, y: u16) -> Option<usize> {
    contains(list, x, y).then(|| offset + usize::from(y - list.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scrolled_hit_testing() {
        assert_eq!(visible_index(Rect::new(1, 4, 38, 8), 10, 2, 7), Some(13));
        assert_eq!(visible_index(Rect::new(1, 4, 38, 8), 10, 0, 7), None);
    }

    #[test]
    fn hit_testing_near_coordinate_limit_does_not_overflow() {
        let bounds = Rect {
            x: u16::MAX - 1,
            y: u16::MAX - 1,
            width: 4,
            height: 4,
        };
        assert!(contains(bounds, u16::MAX, u16::MAX));
        assert!(!contains(bounds, 0, 0));
    }
}
