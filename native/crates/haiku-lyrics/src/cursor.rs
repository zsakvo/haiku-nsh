use crate::Lyrics;

/// Selects the last timestamp at or before the clock, including duplicate timestamps.
/// Callers publish only when `update` returns true; plain text has no moving cursor.
#[derive(Debug, Default)]
pub struct Cursor {
    current: Option<usize>,
}
impl Cursor {
    pub fn current(&self) -> Option<usize> {
        self.current
    }
    pub fn update(&mut self, lyrics: &Lyrics, seconds: f64) -> bool {
        let next = if lyrics.synced && seconds.is_finite() {
            lyrics
                .lines
                .partition_point(|line| line.at <= seconds)
                .checked_sub(1)
        } else {
            None
        };
        let changed = next != self.current;
        self.current = next;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binary_cursor_boundaries_duplicates_seeks_and_change_only() {
        let lyrics = crate::parse("[00:01]one\n[00:03]two\n[00:03]duplicate\n[00:10]last");
        let mut cursor = Cursor::default();
        assert!(!cursor.update(&lyrics, -1.));
        assert!(cursor.update(&lyrics, 1.));
        assert_eq!(cursor.current(), Some(0));
        assert!(!cursor.update(&lyrics, 2.999));
        assert!(cursor.update(&lyrics, 3.));
        assert_eq!(cursor.current(), Some(2));
        assert!(cursor.update(&lyrics, 10000.));
        assert_eq!(cursor.current(), Some(3));
        assert!(cursor.update(&lyrics, 2.));
        assert_eq!(cursor.current(), Some(0));
        assert!(cursor.update(&lyrics, f64::NAN));
        assert_eq!(cursor.current(), None);
        assert!(!cursor.update(&crate::parse("plain\ntext"), 50.));
        assert!(!cursor.update(&crate::Lyrics::default(), 50.));
    }
}
