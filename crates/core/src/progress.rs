//! Progress of running steps, for people watching them. Not the event journal:
//! only the latest state of each review is kept, and it dies with the process.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use uuid::Uuid;

use crate::domain::{EventKind, Progress, ProgressEvent, Severity, Stage};

pub const EVENTS_KEPT: usize = 50;

// shortcut: one entry per review, never evicted; evict finished ones when the
// server runs long enough for that to matter.
#[derive(Default)]
pub struct ProgressBoard(Mutex<HashMap<Uuid, (Instant, Progress)>>);

impl ProgressBoard {
    /// A step begins: the previous step's progress is replaced.
    pub fn start(&self, id: Uuid) {
        let progress = Progress {
            stage: Stage::Extract,
            requirements: 0,
            checked: 0,
            critical: 0,
            major: 0,
            minor: 0,
            events: Vec::new(),
        };
        self.lock().insert(id, (Instant::now(), progress));
    }

    pub fn update(&self, id: Uuid, change: impl FnOnce(&mut Progress)) {
        if let Some((_, progress)) = self.lock().get_mut(&id) {
            change(progress);
        }
    }

    pub fn event(&self, id: Uuid, kind: EventKind, severity: Option<Severity>, text: String) {
        if let Some((started, progress)) = self.lock().get_mut(&id) {
            let at_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            progress.events.push(ProgressEvent {
                kind,
                severity,
                text,
                at_ms,
            });
            let excess = progress.events.len().saturating_sub(EVENTS_KEPT);
            progress.events.drain(..excess);
        }
    }

    pub fn get(&self, id: Uuid) -> Option<Progress> {
        self.lock().get(&id).map(|(_, p)| p.clone())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, (Instant, Progress)>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_step_keeps_its_latest_state_and_the_newest_events() {
        let board = ProgressBoard::default();
        let id = Uuid::nil();
        board.event(id, EventKind::Stage, None, "before start".into());
        assert!(board.get(id).is_none());

        board.start(id);
        board.update(id, |p| p.requirements = 3);
        for n in 0..EVENTS_KEPT + 5 {
            board.event(id, EventKind::Found, Some(Severity::Minor), format!("e{n}"));
        }
        let progress = board.get(id).unwrap();
        assert_eq!(progress.requirements, 3);
        assert_eq!(progress.events.len(), EVENTS_KEPT);
        assert_eq!(progress.events[0].text, "e5");

        board.start(id);
        assert!(board.get(id).unwrap().events.is_empty());
    }
}
