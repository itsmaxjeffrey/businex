//! Minimal cron expression support for job schedules.
//!
//! Five fields: minute hour day-of-month month day-of-week. Supports star,
//! single values, ranges, step values and comma lists. Times are computed in
//! UTC. When both day-of-month and day-of-week are restricted, a time matches
//! when either matches (standard cron behavior).

use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronExpr {
    minutes: BTreeSet<u32>,
    hours: BTreeSet<u32>,
    doms: BTreeSet<u32>,
    months: BTreeSet<u32>,
    dows: BTreeSet<u32>,
    dom_star: bool,
    dow_star: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("invalid cron expression: {0}")]
pub struct CronError(String);

impl CronExpr {
    pub fn parse(expr: &str) -> Result<CronExpr, CronError> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(CronError(format!(
                "expected 5 fields, got {}",
                parts.len()
            )));
        }
        let dom_star = parts[2] == "*";
        let dow_star = parts[4] == "*";
        Ok(CronExpr {
            minutes: parse_field(parts[0], 0, 59)?,
            hours: parse_field(parts[1], 0, 23)?,
            doms: parse_field(parts[2], 1, 31)?,
            months: parse_field(parts[3], 1, 12)?,
            dows: parse_field(parts[4], 0, 6)?,
            dom_star,
            dow_star,
        })
    }

    fn matches(&self, t: DateTime<Utc>) -> bool {
        if !self.minutes.contains(&(t.minute() as u32)) {
            return false;
        }
        if !self.hours.contains(&(t.hour() as u32)) {
            return false;
        }
        if !self.months.contains(&(t.month() as u32)) {
            return false;
        }
        let dom_ok = self.doms.contains(&(t.day() as u32));
        let dow_ok = self.dows.contains(&(t.weekday().num_days_from_sunday() as u32));
        let day_ok = match (self.dom_star, self.dow_star) {
            (true, true) => true,
            (false, true) => dom_ok,
            (true, false) => dow_ok,
            (false, false) => dom_ok || dow_ok,
        };
        day_ok
    }

    /// First matching time strictly after the given instant, if any within a
    /// year of searching.
    pub fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let mut t = after
            .with_second(0)
            .and_then(|t| t.with_nanosecond(0))
            .unwrap_or(after)
            + Duration::minutes(1);
        for _ in 0..(366 * 24 * 60) {
            if self.matches(t) {
                return Some(t);
            }
            t += Duration::minutes(1);
        }
        None
    }
}

fn parse_field(spec: &str, min: u32, max: u32) -> Result<BTreeSet<u32>, CronError> {
    let mut out = BTreeSet::new();
    for part in spec.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => {
                let step: u32 = s
                    .parse()
                    .map_err(|_| CronError(format!("bad step in {}", part)))?;
                if step == 0 {
                    return Err(CronError(format!("step must be positive in {}", part)));
                }
                (r, step)
            }
            None => (part, 1),
        };
        let (lo, hi) = match range {
            "*" => (min, max),
            r => {
                if let Some((a, b)) = r.split_once('-') {
                    (
                        a.parse().map_err(|_| CronError(format!("bad value in {}", part)))?,
                        b.parse().map_err(|_| CronError(format!("bad value in {}", part)))?,
                    )
                } else {
                    let v: u32 = r
                        .parse()
                        .map_err(|_| CronError(format!("bad value in {}", part)))?;
                    (v, v)
                }
            }
        };
        if lo < min || hi > max || lo > hi {
            return Err(CronError(format!("out of range in {}", part)));
        }
        let mut v = lo;
        while v <= hi {
            out.insert(v);
            v += step;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn every_minute() {
        let c = CronExpr::parse("* * * * *").unwrap();
        assert_eq!(c.next_after(t(2026, 1, 1, 0, 0)), Some(t(2026, 1, 1, 0, 1)));
    }

    #[test]
    fn fixed_time_daily() {
        let c = CronExpr::parse("30 6 * * *").unwrap();
        assert_eq!(c.next_after(t(2026, 1, 1, 0, 0)), Some(t(2026, 1, 1, 6, 30)));
        assert_eq!(c.next_after(t(2026, 1, 1, 6, 30)), Some(t(2026, 1, 2, 6, 30)));
    }

    #[test]
    fn step_values() {
        let c = CronExpr::parse("*/15 * * * *").unwrap();
        assert_eq!(c.next_after(t(2026, 1, 1, 0, 7)), Some(t(2026, 1, 1, 0, 15)));
    }

    #[test]
    fn ranges_and_lists() {
        let c = CronExpr::parse("0 9-17 * * 1-5").unwrap();
        // 2026-01-03 is a Saturday: next match is Monday 09:00.
        assert_eq!(c.next_after(t(2026, 1, 3, 12, 0)), Some(t(2026, 1, 5, 9, 0)));
    }

    #[test]
    fn dom_or_dow_when_both_restricted() {
        let c = CronExpr::parse("0 0 1 * 5").unwrap();
        // First of the month OR any Friday; after Fri 2026-01-02 00:00 the
        // next match is the following Friday 2026-01-09 00:00.
        assert_eq!(c.next_after(t(2026, 1, 2, 0, 0)), Some(t(2026, 1, 9, 0, 0)));
    }

    #[test]
    fn rejects_bad_expressions() {
        assert!(CronExpr::parse("* * * *").is_err());
        assert!(CronExpr::parse("61 * * * *").is_err());
        assert!(CronExpr::parse("* * * * 9").is_err());
        assert!(CronExpr::parse("*/0 * * * *").is_err());
    }
}
