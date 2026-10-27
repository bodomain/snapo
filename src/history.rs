//! Wire-compatible with the Python application's immutable revision history.
use anyhow::{Result, bail, ensure};
use chrono::{DateTime, Datelike, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::OpenOptions,
    path::Path,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Session {
    pub date: String,
    pub timestamp: String,
    pub activity: String,
    pub duration_minutes: f64,
    pub comment: String,
}

impl Session {
    pub fn now(activity: impl Into<String>, minutes: f64, comment: impl Into<String>) -> Self {
        let now = Local::now();
        Self {
            date: now.date_naive().to_string(),
            timestamp: now.to_rfc3339(),
            activity: activity.into(),
            duration_minutes: minutes,
            comment: comment.into(),
        }
    }

    pub fn validated(mut self) -> Result<Self> {
        validate_date(&self.date)?;
        // Older Python sessions can contain naive ISO timestamps.
        ensure!(
            DateTime::parse_from_rfc3339(&self.timestamp).is_ok()
                || chrono::NaiveDateTime::parse_from_str(&self.timestamp, "%Y-%m-%dT%H:%M:%S%.f")
                    .is_ok()
                || chrono::NaiveDateTime::parse_from_str(&self.timestamp, "%Y-%m-%dT%H:%M").is_ok()
                || chrono::NaiveDateTime::parse_from_str(&self.timestamp, "%Y-%m-%d %H:%M:%S%.f")
                    .is_ok(),
            "Zeitstempel muss im ISO-Format vorliegen."
        );
        self.activity = self.activity.trim().to_owned();
        ensure!(!self.activity.is_empty(), "Bitte eine Aktivität eingeben.");
        ensure!(
            self.duration_minutes.is_finite() && self.duration_minutes > 0.0,
            "Die Dauer muss größer als null sein."
        );
        Ok(self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Revision {
    pub revision_id: String,
    pub session_id: String,
    pub parents: Vec<String>,
    pub device_id: String,
    pub created_at: String,
    pub session: Session,
    pub deleted: bool,
    pub legacy_id: Option<i64>,
}

impl Revision {
    /// Python's migration deliberately retains legacy payloads verbatim, even
    /// malformed timestamps. Validation applies when a user writes new fields.
    pub fn validate_replication(&self) -> Result<()> {
        ensure!(
            !self.revision_id.is_empty() && !self.session_id.is_empty(),
            "Eine Revisions- oder Sitzungs-ID fehlt."
        );
        if self.legacy_id.is_none() {
            self.session.clone().validated()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub session_id: String,
    pub heads: Vec<Revision>,
}

impl Entry {
    pub fn conflicted(&self) -> bool {
        self.heads.len() != 1
    }
    pub fn current(&self) -> Option<&Revision> {
        if self.conflicted() {
            None
        } else {
            self.heads.first()
        }
    }
    pub fn head_ids(&self) -> Vec<String> {
        self.heads.iter().map(|r| r.revision_id.clone()).collect()
    }
    pub fn active(&self) -> Option<&Session> {
        self.current().filter(|r| !r.deleted).map(|r| &r.session)
    }
}

pub fn reconstruct(revisions: Vec<Revision>) -> Vec<Entry> {
    let mut grouped: HashMap<String, Vec<Revision>> = HashMap::new();
    for revision in revisions {
        grouped
            .entry(revision.session_id.clone())
            .or_default()
            .push(revision);
    }
    let mut entries = Vec::new();
    for (session_id, versions) in grouped {
        let consumed: HashSet<_> = versions
            .iter()
            .flat_map(|r| r.parents.iter().cloned())
            .collect();
        let mut heads: Vec<_> = versions
            .into_iter()
            .filter(|r| !consumed.contains(&r.revision_id))
            .collect();
        heads.sort_by(|a, b| a.revision_id.cmp(&b.revision_id));
        if !heads.is_empty() {
            entries.push(Entry { session_id, heads });
        }
    }
    entries.sort_by_cached_key(|e| {
        std::cmp::Reverse((
            e.heads
                .iter()
                .map(|r| &r.session.timestamp)
                .max()
                .cloned()
                .unwrap_or_default(),
            e.session_id.clone(),
        ))
    });
    entries
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    History,
    Trash,
    Conflicts,
}

#[derive(Clone, Default)]
pub struct Filter {
    pub search: String,
    pub activity: String,
    pub since: String,
    pub until: String,
}

impl Filter {
    pub fn validate(&self) -> Result<()> {
        for date in [&self.since, &self.until] {
            if !date.is_empty() {
                validate_date(date)?;
            }
        }
        if !self.since.is_empty() && !self.until.is_empty() && self.since > self.until {
            bail!("Das Startdatum muss vor dem Enddatum liegen.");
        }
        Ok(())
    }
    pub fn matches(&self, entry: &Entry, view: View) -> bool {
        let visible = match view {
            View::History => entry.conflicted() || entry.active().is_some(),
            View::Trash => entry.current().is_some_and(|r| r.deleted),
            View::Conflicts => entry.conflicted(),
        };
        visible
            && entry.heads.iter().any(|r| {
                let s = &r.session;
                (self.since.is_empty() || s.date >= self.since)
                    && (self.until.is_empty() || s.date <= self.until)
                    && s.activity
                        .to_lowercase()
                        .contains(&self.activity.to_lowercase())
                    && format!("{} {} {}", s.activity, s.comment, s.timestamp)
                        .to_lowercase()
                        .contains(&self.search.to_lowercase())
            })
    }
}

fn validate_date(date: &str) -> Result<()> {
    let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("Datum bitte als JJJJ-MM-TT eingeben."))?;
    ensure!(
        parsed.to_string() == date,
        "Datum bitte als JJJJ-MM-TT eingeben."
    );
    Ok(())
}

pub type Totals = BTreeMap<(String, String), f64>;
pub fn totals(entries: &[Entry], weekly: bool) -> Totals {
    let mut result = BTreeMap::new();
    for session in entries.iter().filter_map(Entry::active) {
        let period = if weekly {
            let Ok(day) = NaiveDate::parse_from_str(&session.date, "%Y-%m-%d") else {
                continue;
            };
            let week = day.iso_week();
            format!("{}-W{:02}", week.year(), week.week())
        } else {
            session.date.clone()
        };
        *result
            .entry((period, session.activity.clone()))
            .or_default() += session.duration_minutes;
    }
    result
}

pub fn export_csv(entries: &[Entry], path: &Path) -> Result<usize> {
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut writer = csv::Writer::from_writer(file);
    writer.write_record([
        "session_id",
        "date",
        "timestamp",
        "activity",
        "duration_minutes",
        "comment",
    ])?;
    let safe = |s: &str| {
        if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
            format!("'{s}")
        } else {
            s.to_owned()
        }
    };
    let mut count = 0;
    for entry in entries {
        if let Some(s) = entry.active() {
            writer.write_record([
                entry.session_id.clone(),
                s.date.clone(),
                s.timestamp.clone(),
                safe(&s.activity),
                s.duration_minutes.to_string(),
                safe(&s.comment),
            ])?;
            count += 1;
        }
    }
    writer.flush()?;
    Ok(count)
}
