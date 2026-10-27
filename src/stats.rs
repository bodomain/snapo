//! Pure aggregations for the Stats page. Works on active sessions only; the
//! callers already apply the history filters and exclude trash/conflicts.

use crate::history::Session;
use chrono::{DateTime, Datelike, NaiveDate, Timelike};
use std::collections::BTreeMap;

/// Headline figures shown as tiles above the charts.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub total_minutes: f64,
    pub sessions: usize,
    pub activities: usize,
    pub active_days: usize,
    pub average_per_active_day: f64,
    pub best_day: Option<(String, f64)>,
    pub streak: u32,
}

fn day(session: &Session) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&session.date, "%Y-%m-%d").ok()
}

fn minutes_per_day(sessions: &[&Session]) -> BTreeMap<String, f64> {
    let mut result: BTreeMap<String, f64> = BTreeMap::new();
    for session in sessions {
        *result.entry(session.date.clone()).or_default() += session.duration_minutes;
    }
    result
}

pub fn summarize(sessions: &[&Session]) -> Summary {
    let total_minutes: f64 = sessions.iter().map(|s| s.duration_minutes).sum();
    let by_day = minutes_per_day(sessions);
    let activities: std::collections::BTreeSet<_> =
        sessions.iter().map(|s| s.activity.as_str()).collect();
    let best_day = by_day
        .iter()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(date, minutes)| (date.clone(), *minutes));
    let streak = {
        let dates: std::collections::BTreeSet<NaiveDate> =
            by_day.keys().filter_map(|d| day_key(d)).collect();
        let mut streak = 0;
        if let Some(mut cursor) = dates.iter().next_back().copied() {
            while dates.contains(&cursor) {
                streak += 1;
                let Some(previous) = cursor.pred_opt() else {
                    break;
                };
                cursor = previous;
            }
        }
        streak
    };
    let active_days = by_day.len();
    Summary {
        total_minutes,
        sessions: sessions.len(),
        activities: activities.len(),
        active_days,
        average_per_active_day: if active_days == 0 {
            0.0
        } else {
            total_minutes / active_days as f64
        },
        best_day,
        streak,
    }
}

fn day_key(date: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

/// Calendar month (1..=12) → activity → minutes, restricted to `year`.
pub fn by_month(sessions: &[&Session], year: i32) -> BTreeMap<u32, BTreeMap<String, f64>> {
    let mut result: BTreeMap<u32, BTreeMap<String, f64>> = BTreeMap::new();
    for session in sessions {
        let Some(date) = day(session) else { continue };
        if date.year() != year {
            continue;
        }
        *result
            .entry(date.month())
            .or_default()
            .entry(session.activity.clone())
            .or_default() += session.duration_minutes;
    }
    result
}

/// ISO week → activity → minutes, restricted to weeks whose ISO year is `year`.
pub fn by_week(sessions: &[&Session], year: i32) -> BTreeMap<u32, BTreeMap<String, f64>> {
    let mut result: BTreeMap<u32, BTreeMap<String, f64>> = BTreeMap::new();
    for session in sessions {
        let Some(date) = day(session) else { continue };
        let week = date.iso_week();
        if week.year() != year {
            continue;
        }
        *result
            .entry(week.week())
            .or_default()
            .entry(session.activity.clone())
            .or_default() += session.duration_minutes;
    }
    result
}

/// Minutes per weekday; index 0 is Monday, 6 is Sunday.
pub fn by_weekday(sessions: &[&Session]) -> [f64; 7] {
    let mut result = [0.0; 7];
    for session in sessions {
        let Some(date) = day(session) else { continue };
        result[date.weekday().num_days_from_monday() as usize] += session.duration_minutes;
    }
    result
}

/// Minutes per hour 0..=23, taken from the recorded timestamp.
pub fn by_hour(sessions: &[&Session]) -> [f64; 24] {
    let mut result = [0.0; 24];
    for session in sessions {
        let Some(hour) = hour(session) else { continue };
        result[hour as usize] += session.duration_minutes;
    }
    result
}

fn hour(session: &Session) -> Option<u32> {
    DateTime::parse_from_rfc3339(&session.timestamp)
        .ok()
        .map(|t| t.hour())
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(&session.timestamp, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|t| t.hour())
        })
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(&session.timestamp, "%Y-%m-%d %H:%M:%S%.f")
                .ok()
                .map(|t| t.hour())
        })
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(&session.timestamp, "%Y-%m-%dT%H:%M")
                .ok()
                .map(|t| t.hour())
        })
}

/// Activities ranked by total minutes, most first.
pub fn by_activity(sessions: &[&Session]) -> Vec<(String, f64)> {
    let mut totals: BTreeMap<String, f64> = BTreeMap::new();
    for session in sessions {
        *totals.entry(session.activity.clone()).or_default() += session.duration_minutes;
    }
    let mut ranked: Vec<(String, f64)> = totals.into_iter().collect();
    ranked.sort_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(activity: &str, date: &str, timestamp: &str, minutes: f64) -> Session {
        Session {
            date: date.into(),
            timestamp: timestamp.into(),
            activity: activity.into(),
            duration_minutes: minutes,
            comment: String::new(),
        }
    }

    #[test]
    fn summarize_aggregates_totals_days_activities_and_streak() {
        let sessions = [
            session("Lesen", "2026-03-01", "2026-03-01T09:00:00+01:00", 30.0),
            session("Lesen", "2026-03-01", "2026-03-01T14:00:00+01:00", 15.0),
            session("Schreiben", "2026-03-02", "2026-03-02T10:00:00+01:00", 45.0),
            session("Lernen", "2026-03-04", "2026-03-04T11:00:00+01:00", 60.0),
        ];
        let refs: Vec<&Session> = sessions.iter().collect();
        let summary = summarize(&refs);
        assert_eq!(summary.total_minutes, 150.0);
        assert_eq!(summary.sessions, 4);
        assert_eq!(summary.activities, 3);
        assert_eq!(summary.active_days, 3);
        assert!((summary.average_per_active_day - 50.0).abs() < 1e-9);
        assert_eq!(summary.best_day, Some(("2026-03-04".into(), 60.0)));
        assert_eq!(
            summary.streak, 1,
            "01.–02. sind aufeinanderfolgend, 04. bricht"
        );
    }

    #[test]
    fn month_and_week_split_the_calendar_year() {
        let sessions = [
            session("Lesen", "2026-01-15", "2026-01-15T09:00:00+01:00", 10.0),
            session("Lesen", "2026-02-01", "2026-02-01T09:00:00+01:00", 20.0),
            session("Schreiben", "2026-02-20", "2026-02-20T09:00:00+01:00", 30.0),
            session("Lesen", "2025-12-31", "2025-12-31T09:00:00+01:00", 99.0),
        ];
        let refs: Vec<&Session> = sessions.iter().collect();
        let months = by_month(&refs, 2026);
        assert_eq!(months[&1]["Lesen"], 10.0);
        assert_eq!(months[&2]["Lesen"], 20.0);
        assert_eq!(months[&2]["Schreiben"], 30.0);
        assert!(
            !months.contains_key(&12),
            "Dezember 2025 gehört nicht zu 2026"
        );
        let weeks = by_week(&refs, 2026);
        // 2025-12-31 belongs to ISO week 2026-W01, so it counts for 2026.
        assert_eq!(weeks.values().flat_map(|m| m.values()).sum::<f64>(), 159.0);
    }

    #[test]
    fn weekday_and_hour_use_the_recorded_time() {
        // 2026-03-02 is a Monday; the two timestamps fall into hours 9 and 22.
        let sessions = [
            session("Lesen", "2026-03-02", "2026-03-02T09:00:00+01:00", 30.0),
            session("Lesen", "2026-03-02", "2026-03-02T22:00:00+01:00", 10.0),
            session("Lesen", "2026-03-03", "2026-03-03T09:00:00+01:00", 20.0),
        ];
        let refs: Vec<&Session> = sessions.iter().collect();
        let weekdays = by_weekday(&refs);
        assert_eq!(weekdays[0], 40.0, "Montag");
        assert_eq!(weekdays[1], 20.0, "Dienstag");
        assert_eq!(weekdays.iter().sum::<f64>(), 60.0);
        let hours = by_hour(&refs);
        assert_eq!(hours[9], 50.0);
        assert_eq!(hours[22], 10.0);
    }

    #[test]
    fn activity_ranking_sorts_descending() {
        let sessions = [
            session("A", "2026-01-01", "2026-01-01T09:00:00+01:00", 10.0),
            session("B", "2026-01-01", "2026-01-01T10:00:00+01:00", 40.0),
            session("A", "2026-01-01", "2026-01-01T11:00:00+01:00", 20.0),
        ];
        let refs: Vec<&Session> = sessions.iter().collect();
        assert_eq!(
            by_activity(&refs),
            vec![("B".to_string(), 40.0), ("A".to_string(), 30.0)]
        );
    }
}
