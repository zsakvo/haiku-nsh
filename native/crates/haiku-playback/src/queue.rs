use haiku_model::{Error, ErrorCode, Track};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Sequential,
    RepeatAll,
    RepeatOne,
    Shuffle,
}

/// Positions, rather than track ids, distinguish duplicate entries.
#[derive(Clone, Debug)]
pub struct Queue {
    pub tracks: Vec<Track>,
    pub index: i32,
    pub mode: Mode,
    order: Vec<usize>,
    cursor: usize,
    seed: u64,
}
impl Default for Queue {
    fn default() -> Self {
        Self::with_seed(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
        )
    }
}
impl Queue {
    pub fn with_seed(seed: u64) -> Self {
        Self {
            tracks: vec![],
            index: -1,
            mode: Mode::Sequential,
            order: vec![],
            cursor: 0,
            seed: seed.max(1),
        }
    }
    pub fn current(&self) -> Option<&Track> {
        self.tracks.get(self.index as usize)
    }
    fn shuffled(&mut self, mut entries: Vec<usize>) -> Vec<usize> {
        for i in (1..entries.len()).rev() {
            self.seed ^= self.seed << 13;
            self.seed ^= self.seed >> 7;
            self.seed ^= self.seed << 17;
            entries.swap(i, (self.seed % (i as u64 + 1)) as usize);
        }
        entries
    }
    fn reset_shuffle(&mut self) {
        self.order.clear();
        self.cursor = 0;
        if self.index >= 0 {
            let rest = self.shuffled(
                (0..self.tracks.len())
                    .filter(|i| *i != self.index as usize)
                    .collect(),
            );
            self.order.push(self.index as usize);
            self.order.extend(rest);
        }
    }
    pub fn replace(&mut self, tracks: Vec<Track>, index: i32) {
        self.tracks = tracks;
        self.index = if self.tracks.is_empty() {
            -1
        } else {
            index.clamp(0, self.tracks.len() as i32 - 1)
        };
        self.reset_shuffle();
    }
    pub fn clear(&mut self) {
        self.replace(vec![], 0);
    }
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.reset_shuffle();
    }
    pub fn jump(&mut self, index: i32) -> Result<(), Error> {
        if index < 0 || index as usize >= self.tracks.len() {
            return Err(Error::new(
                ErrorCode::InvalidArgument,
                "Invalid queue position",
            ));
        }
        self.index = index;
        self.reset_shuffle();
        Ok(())
    }
    pub fn append(&mut self, tracks: Vec<Track>) {
        let begin = self.tracks.len();
        self.tracks.extend(tracks);
        let added = self.shuffled((begin..self.tracks.len()).collect());
        self.order.extend(added);
        if self.index < 0 && !self.tracks.is_empty() {
            self.index = 0;
            self.reset_shuffle();
        }
    }
    pub fn play_next(&mut self, track: Track) {
        if self.index < 0 {
            self.replace(vec![track], 0);
            return;
        }
        let at = self.index as usize + 1;
        self.tracks.insert(at, track);
        for i in &mut self.order {
            if *i >= at {
                *i += 1;
            }
        }
        self.order.insert(self.cursor + 1, at);
    }
    pub fn replace_after_current(&mut self, tracks: Vec<Track>) {
        if self.index < 0 {
            return;
        }
        let current = self.index as usize;
        let prefix: Vec<_> = self.order[..=self.cursor]
            .iter()
            .copied()
            .filter(|i| *i <= current)
            .collect();
        let future: Vec<_> = self.order[self.cursor + 1..]
            .iter()
            .copied()
            .filter(|i| *i <= current)
            .collect();
        self.tracks.truncate(current + 1);
        self.tracks.extend(tracks);
        self.cursor = prefix.len() - 1;
        self.order = prefix;
        self.order.extend(future);
        let added = self.shuffled((current + 1..self.tracks.len()).collect());
        self.order.extend(added);
    }
    pub fn remove(&mut self, at: i32) {
        if at < 0 || at as usize >= self.tracks.len() {
            return;
        }
        self.tracks.remove(at as usize);
        if self.tracks.is_empty() {
            self.clear();
            return;
        }
        if at < self.index {
            self.index -= 1;
        } else if at == self.index {
            self.index = at.min(self.tracks.len() as i32 - 1);
        }
        self.reset_shuffle();
    }
    pub fn move_item(&mut self, from: i32, to: i32) {
        if from < 0
            || to < 0
            || from as usize >= self.tracks.len()
            || to as usize >= self.tracks.len()
        {
            return;
        }
        let mut identities: Vec<_> = (0..self.tracks.len()).collect();
        let identity = identities.remove(from as usize);
        identities.insert(to as usize, identity);
        let remap = |old| identities.iter().position(|i| *i == old).unwrap();
        let track = self.tracks.remove(from as usize);
        self.tracks.insert(to as usize, track);
        self.index = remap(self.index as usize) as i32;
        self.order = self.order.iter().map(|i| remap(*i)).collect();
    }
    pub fn peek_next(&mut self, natural: bool) -> Option<i32> {
        if self.current().is_none() {
            return None;
        }
        if natural && self.mode == Mode::RepeatOne {
            return Some(self.index);
        }
        if self.mode == Mode::Shuffle {
            if self.cursor + 1 == self.order.len() {
                let mut round = self.shuffled((0..self.tracks.len()).collect());
                if round.len() > 1 && round[0] == self.index as usize {
                    round.swap(0, 1);
                }
                self.order.extend(round);
            }
            return self.order.get(self.cursor + 1).map(|i| *i as i32);
        }
        if self.index + 1 < self.tracks.len() as i32 {
            Some(self.index + 1)
        } else if self.mode == Mode::Sequential {
            None
        } else {
            Some(0)
        }
    }
    pub fn next(&mut self, natural: bool) -> Option<i32> {
        let next = self.peek_next(natural)?;
        self.index = next;
        if self.mode == Mode::Shuffle {
            self.cursor += 1;
        }
        Some(next)
    }
    pub fn previous(&mut self) -> Option<i32> {
        self.current()?;
        if self.mode == Mode::Shuffle {
            if self.cursor > 0 {
                self.cursor -= 1;
                self.index = self.order[self.cursor] as i32;
            }
        } else if self.index > 0 {
            self.index -= 1;
        } else if self.mode != Mode::Sequential {
            self.index = self.tracks.len() as i32 - 1;
        }
        Some(self.index)
    }
}
