//! What Archie says on its own while it works: a queue the conversation reads when the room
//! is quiet, like Sentry's notices, and a subtitle the stage shows while it is said.
//!
//! Narration is Archie's own words about what it is doing — never what a person said, never
//! what a window showed — so it may be printed on the screen. A queue that has fallen behind
//! keeps every milestone but only the latest progress line: a voice catching up on "a
//! quarter done" after "halfway" would be saying something that is no longer true.

use crate::draw::{Narrate, Said};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// The most lines kept waiting to be said.
pub const MOST_WAITING: usize = 8;

#[derive(Clone, Default)]
pub struct Narration {
    waiting: Arc<Mutex<VecDeque<Said>>>,
    subtitles: Option<PathBuf>,
}

impl Narration {
    pub fn new(subtitles: Option<PathBuf>) -> Self {
        Self {
            waiting: Arc::default(),
            subtitles,
        }
    }

    /// Adds a line to say. A progress line replaces any progress line still waiting.
    pub fn push(&self, said: Said) {
        let Ok(mut waiting) = self.waiting.lock() else {
            return;
        };
        if said.progress {
            waiting.retain(|line| !line.progress);
        }
        waiting.push_back(said);
        while waiting.len() > MOST_WAITING {
            // The oldest progress line goes first, then the oldest line of any kind.
            match waiting.iter().position(|line| line.progress) {
                Some(at) => {
                    waiting.remove(at);
                }
                None => {
                    waiting.pop_front();
                }
            }
        }
    }

    /// The next line to say, if any.
    pub fn next(&self) -> Option<Said> {
        self.waiting.lock().ok()?.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.waiting.lock().map_or(true, |w| w.is_empty())
    }

    /// Shows `text` under the stage while it is said: written whole to a file the stage
    /// reads, replacing what was there. An empty text clears it.
    pub fn subtitle(&self, text: &str, lang: &str) {
        let Some(path) = &self.subtitles else {
            return;
        };
        let at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let body = serde_json::json!({ "text": text, "lang": lang, "at_ms": at_ms }).to_string();
        let staged = path.with_extension("tmp");
        if std::fs::write(&staged, body).is_ok() {
            let _ = std::fs::rename(&staged, path);
        }
    }
}

impl Narrate for Narration {
    fn tell(&mut self, said: Said) {
        self.push(said);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::Lang;

    fn line(text: &str, progress: bool) -> Said {
        Said {
            text: text.into(),
            progress,
            lang: Lang::En,
        }
    }

    #[test]
    fn only_the_latest_progress_waits() {
        let n = Narration::new(None);
        n.push(line("opening", false));
        n.push(line("a quarter", true));
        n.push(line("second ink", false));
        n.push(line("halfway", true));
        let said: Vec<String> = std::iter::from_fn(|| n.next()).map(|s| s.text).collect();
        assert_eq!(said, ["opening", "second ink", "halfway"]);
        assert!(n.is_empty());
    }

    #[test]
    fn a_queue_that_falls_far_behind_keeps_its_milestones() {
        let n = Narration::new(None);
        for i in 0..20 {
            n.push(line(&format!("m{i}"), false));
        }
        let said: Vec<String> = std::iter::from_fn(|| n.next()).map(|s| s.text).collect();
        assert_eq!(said.len(), MOST_WAITING);
        assert_eq!(said.last().map(String::as_str), Some("m19"));
    }

    #[test]
    fn a_subtitle_is_written_whole() {
        let dir = std::env::temp_dir().join(format!("archie-subtitle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("narration.json");
        let n = Narration::new(Some(file.clone()));
        n.subtitle("Now the second ink.", "en");
        let read: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(read["text"], "Now the second ink.");
        assert_eq!(read["lang"], "en");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
