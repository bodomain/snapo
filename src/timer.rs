use crate::history::Session;
use anyhow::{Result, ensure};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Plan {
    pub work: u32,
    pub short_break: u32,
    pub long_break: u32,
    pub cycles: u32,
    pub endless: bool,
    pub sound: bool,
}
impl Default for Plan {
    fn default() -> Self {
        Self {
            work: 25,
            short_break: 5,
            long_break: 15,
            cycles: 4,
            endless: false,
            sound: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Work,
    ShortBreak,
    LongBreak,
}
impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Work => "Fokuszeit",
            Self::ShortBreak => "Kurze Pause",
            Self::LongBreak => "Lange Pause",
        }
    }
}

pub struct Clock {
    pub id: String,
    pub session: Session,
    pub target: Option<Duration>,
    pub paused: bool,
    since: Instant,
    accumulated: Duration,
    frozen: Option<Session>,
}

impl Clock {
    pub fn new(
        activity: String,
        comment: String,
        minutes: Option<u32>,
        now: Instant,
    ) -> Result<Self> {
        ensure!(
            !activity.trim().is_empty(),
            "Bitte eine Aktivität eingeben."
        );
        ensure!(minutes != Some(0), "Die Dauer muss größer als null sein.");
        Ok(Self {
            id: Uuid::new_v4().to_string(),
            session: Session::now(activity.trim(), 1.0, comment),
            target: minutes.map(|m| Duration::from_secs(u64::from(m) * 60)),
            paused: false,
            since: now,
            accumulated: Duration::ZERO,
            frozen: None,
        })
    }
    pub fn elapsed(&self, now: Instant) -> Duration {
        let elapsed = self.accumulated
            + if self.paused {
                Duration::ZERO
            } else {
                now.saturating_duration_since(self.since)
            };
        self.target.map_or(elapsed, |target| elapsed.min(target))
    }
    pub fn complete(&self, now: Instant) -> bool {
        self.target.is_some_and(|t| self.elapsed(now) >= t)
    }
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        self.target.map(|t| t.saturating_sub(self.elapsed(now)))
    }
    pub fn toggle_pause(&mut self, now: Instant) {
        if self.frozen.is_some() {
            return;
        }
        if self.paused {
            self.since = now;
            self.paused = false;
        } else {
            self.accumulated = self.elapsed(now);
            self.paused = true;
        }
    }
    /// Freeze before persistence, so a retry has identical contents and ID.
    pub fn snapshot(&mut self, now: Instant) -> Result<Session> {
        if let Some(session) = &self.frozen {
            return Ok(session.clone());
        }
        let elapsed = self.elapsed(now);
        ensure!(
            elapsed > Duration::ZERO,
            "Es gibt noch keine Fokuszeit zu speichern."
        );
        self.accumulated = elapsed;
        self.paused = true;
        let mut session = self.session.clone();
        session.duration_minutes = elapsed.as_secs_f64() / 60.0;
        self.frozen = Some(session.clone());
        Ok(session)
    }
}

pub struct Run {
    pub plan: Plan,
    pub phase: Phase,
    pub cycle: u32,
    pub clock: Clock,
}
impl Run {
    pub fn new(plan: Plan, activity: String, comment: String, now: Instant) -> Result<Self> {
        ensure!(
            (1..=240).contains(&plan.work)
                && (1..=120).contains(&plan.short_break)
                && (1..=180).contains(&plan.long_break)
                && (1..=24).contains(&plan.cycles),
            "Ungültige Timer-Einstellungen."
        );
        let clock = Clock::new(
            activity,
            comment,
            if plan.endless { None } else { Some(plan.work) },
            now,
        )?;
        Ok(Self {
            plan,
            phase: Phase::Work,
            cycle: 1,
            clock,
        })
    }
    /// Called only after a completed work interval has been committed successfully.
    pub fn advance(&mut self, now: Instant) -> Result<bool> {
        if self.phase == Phase::Work {
            if self.plan.endless || self.cycle >= self.plan.cycles {
                return Ok(false);
            }
            self.phase = if self.cycle.is_multiple_of(4) {
                Phase::LongBreak
            } else {
                Phase::ShortBreak
            };
        } else {
            self.phase = Phase::Work;
            self.cycle += 1;
        }
        let minutes = match self.phase {
            Phase::Work => self.plan.work,
            Phase::ShortBreak => self.plan.short_break,
            Phase::LongBreak => self.plan.long_break,
        };
        self.clock = Clock::new(
            self.clock.session.activity.clone(),
            self.clock.session.comment.clone(),
            Some(minutes),
            now,
        )?;
        Ok(true)
    }
}

pub fn bell() {
    std::thread::spawn(|| {
        if let Ok(mut stream) = rodio::OutputStreamBuilder::open_default_stream() {
            stream.log_on_drop(false);
            if let Ok(source) = rodio::Decoder::try_from(std::io::Cursor::new(
                include_bytes!("../bell.wav").as_slice(),
            )) {
                let sink = rodio::Sink::connect_new(stream.mixer());
                sink.append(source);
                sink.sleep_until_end();
                return;
            }
        }
        // A missing audio device must not block the timer or crash the app.
        use std::io::Write;
        let _ = std::io::stderr().write_all(b"\x07");
    });
}

/// A background bell that rings on time even when the egui loop is throttled.
///
/// On Wayland (e.g. under Sway) a window on a hidden workspace receives no
/// frame callbacks, so winit throttles redraws and `App::update` – including
/// the tick loop – stalls until the window becomes visible again. The timer
/// itself stays correct, because `Clock` measures elapsed time from `Instant`
/// deltas, but a bell driven by the tick loop would ring late. The watcher
/// owns no timer state: the UI publishes the current deadline once per frame,
/// and while the loop is stalled nothing can change the run, so the last
/// published deadline stays valid.
pub struct DeadlineWatcher {
    shared: Arc<(Mutex<Deadline>, Condvar)>,
}

#[derive(Default)]
struct Deadline {
    /// Bumped on every schedule/cancel, so stale deadlines never ring.
    generation: u64,
    at: Option<Instant>,
    /// Generation the watcher already rang for, so the tick loop can skip
    /// its own bell when it catches up after a stalled loop.
    rung: Option<u64>,
}

impl DeadlineWatcher {
    /// Rings the default [`bell`] when the deadline is reached.
    pub fn spawn() -> Self {
        Self::with_bell(bell)
    }
    /// Uses `ring` instead of [`bell`], which keeps the watcher testable
    /// without an audio device.
    pub fn with_bell(ring: impl Fn() + Send + 'static) -> Self {
        let shared = Arc::new((Mutex::new(Deadline::default()), Condvar::new()));
        let state = Arc::clone(&shared);
        std::thread::spawn(move || watch(state, ring));
        Self { shared }
    }
    /// Publishes when the current phase should ring; a deadline already in
    /// the past rings immediately, because the phase ended while the loop
    /// was stalled.
    pub fn schedule(&self, at: Instant) {
        publish(&self.shared, Some(at));
    }
    /// Removes the deadline, e.g. after a pause, discard or finished run.
    pub fn cancel(&self) {
        publish(&self.shared, None);
    }
    /// Takes whether the watcher already rang for the current phase, so the
    /// tick loop does not ring a second time when it catches up.
    pub fn take_rung(&self) -> bool {
        self.shared.0.lock().unwrap().rung.take().is_some()
    }
    /// The currently published deadline.
    pub fn deadline(&self) -> Option<Instant> {
        self.shared.0.lock().unwrap().at
    }
}

fn publish(shared: &(Mutex<Deadline>, Condvar), at: Option<Instant>) {
    let (mutex, wake) = shared;
    let mut deadline = mutex.lock().unwrap();
    deadline.generation += 1;
    deadline.at = at;
    deadline.rung = None;
    wake.notify_all();
}

fn watch(shared: Arc<(Mutex<Deadline>, Condvar)>, ring: impl Fn()) {
    let (mutex, wake) = &*shared;
    let mut deadline = mutex.lock().unwrap();
    loop {
        let Some(at) = deadline.at else {
            deadline = wake.wait(deadline).unwrap();
            continue;
        };
        let generation = deadline.generation;
        // Wait until the deadline or a reschedule/cancel (generation change).
        // `wait_timeout` also returns on spurious wakeups, so re-check and
        // keep waiting with the remaining time.
        let mut remaining = at.saturating_duration_since(Instant::now());
        let reached = if remaining > Duration::ZERO {
            loop {
                let (guard, result) = wake.wait_timeout(deadline, remaining).unwrap();
                deadline = guard;
                if deadline.generation != generation {
                    break false;
                }
                if result.timed_out() {
                    break true;
                }
                remaining = at.saturating_duration_since(Instant::now());
                if remaining == Duration::ZERO {
                    break true;
                }
            }
        } else {
            true
        };
        if reached && deadline.rung != Some(generation) {
            deadline.rung = Some(generation);
            drop(deadline);
            ring();
            // Park until the UI publishes the next deadline, so an already
            // reached deadline never rings twice.
            deadline = mutex.lock().unwrap();
            while deadline.generation == generation && deadline.at == Some(at) {
                deadline = wake.wait(deadline).unwrap();
            }
        }
    }
}

pub fn greeting() -> &'static str {
    use chrono::Timelike;
    match Local::now().hour() {
        5..=10 => "Guten Morgen.",
        11..=17 => "Guten Tag.",
        _ => "Guten Abend.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    fn counted() -> (DeadlineWatcher, std::sync::Arc<AtomicUsize>) {
        let rings = std::sync::Arc::new(AtomicUsize::new(0));
        let counted = std::sync::Arc::clone(&rings);
        (
            DeadlineWatcher::with_bell(move || {
                counted.fetch_add(1, Ordering::Relaxed);
            }),
            rings,
        )
    }

    fn await_rings(rings: &std::sync::Arc<AtomicUsize>, expected: usize) {
        for _ in 0..250 {
            if rings.load(Ordering::Relaxed) >= expected {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn cancelled_deadlines_never_ring() {
        let (watcher, rings) = counted();
        watcher.schedule(Instant::now() + Duration::from_millis(10));
        watcher.cancel();
        assert_eq!(watcher.deadline(), None);
        thread::sleep(Duration::from_millis(150));
        assert_eq!(rings.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn deadline_in_the_past_rings_exactly_once() {
        let (watcher, rings) = counted();
        watcher.schedule(Instant::now() - Duration::from_secs(1));
        await_rings(&rings, 1);
        thread::sleep(Duration::from_millis(150));
        assert_eq!(
            rings.load(Ordering::Relaxed),
            1,
            "Ein Termin klingelt nur einmal"
        );
        assert!(
            watcher.take_rung(),
            "Der Watcher meldet dem UI, dass er bereits geklingelt hat"
        );
        assert!(!watcher.take_rung());
    }

    #[test]
    fn rescheduling_replaces_the_pending_deadline() {
        let (watcher, rings) = counted();
        watcher.schedule(Instant::now() + Duration::from_millis(10));
        watcher.schedule(Instant::now() + Duration::from_secs(3600));
        thread::sleep(Duration::from_millis(150));
        assert_eq!(
            rings.load(Ordering::Relaxed),
            0,
            "Nur der letzte Termin zählt"
        );
    }
}
