//! One-shot, interval, and timezone-aware cron schedule calculation.

use std::str::FromStr;

use chrono::{DateTime, TimeZone as _, Utc};
use chrono_tz::Tz;
use cron::Schedule;

use crate::{AutomationError, AutomationErrorKind, AutomationResult};

/// Behavior when an always script starts after a scheduled occurrence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissedRunPolicy {
    Skip,
    RunOnce,
}

/// Absolute instant plus its timezone-local representation for diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledInstant {
    pub unix_ms: u64,
    pub local_rfc3339: String,
}

/// Parsed cron expression and explicit IANA timezone.
#[derive(Clone, Debug)]
pub struct CronSchedule {
    expression: String,
    timezone_name: String,
    schedule: Schedule,
    timezone: Tz,
}

impl CronSchedule {
    /// Parses a seven-field cron expression and IANA timezone name.
    ///
    /// # Errors
    ///
    /// Returns an input error for an invalid expression or timezone.
    pub fn parse(expression: &str, timezone_name: &str) -> AutomationResult<Self> {
        let schedule = Schedule::from_str(expression).map_err(|error| {
            AutomationError::new(
                AutomationErrorKind::InvalidInput,
                "schedule.cron.parse",
                false,
                "The cron expression is invalid",
            )
            .with_safe_detail(format!("parser={error}"))
        })?;
        let timezone = Tz::from_str(timezone_name).map_err(|_| {
            AutomationError::new(
                AutomationErrorKind::InvalidInput,
                "schedule.timezone.parse",
                false,
                "The schedule timezone is invalid",
            )
        })?;
        Ok(Self {
            expression: expression.into(),
            timezone_name: timezone_name.into(),
            schedule,
            timezone,
        })
    }

    /// Returns the source cron expression.
    #[must_use]
    pub fn expression(&self) -> &str {
        &self.expression
    }

    /// Returns the explicit IANA timezone name.
    #[must_use]
    pub fn timezone_name(&self) -> &str {
        &self.timezone_name
    }

    /// Finds the first occurrence strictly after a Unix-millisecond instant.
    ///
    /// # Errors
    ///
    /// Returns an input error if the timestamp is outside Chrono's supported range or
    /// the schedule has no later occurrence.
    pub fn next_after(&self, after_unix_ms: u64) -> AutomationResult<ScheduledInstant> {
        let after_utc = utc_from_millis(after_unix_ms)?;
        let after_local = after_utc.with_timezone(&self.timezone);
        let next = self.schedule.after(&after_local).next().ok_or_else(|| {
            AutomationError::new(
                AutomationErrorKind::InvalidInput,
                "schedule.cron.next",
                false,
                "The cron schedule has no later occurrence",
            )
        })?;
        let unix_ms = u64::try_from(next.timestamp_millis()).map_err(|_| invalid_timestamp())?;
        Ok(ScheduledInstant {
            unix_ms,
            local_rfc3339: next.to_rfc3339(),
        })
    }
}

/// Supported persisted scheduling modes.
#[derive(Clone, Debug)]
pub enum SchedulePlan {
    Once { at_unix_ms: u64 },
    Interval { anchor_unix_ms: u64, every_ms: u64 },
    Cron(Box<CronSchedule>),
}

impl SchedulePlan {
    /// Returns the first occurrence strictly after an instant.
    ///
    /// # Errors
    ///
    /// Returns an input error for a zero interval, an exhausted one-shot schedule,
    /// invalid timestamps, or an exhausted cron expression.
    pub fn next_after(&self, after_unix_ms: u64) -> AutomationResult<Option<ScheduledInstant>> {
        match self {
            Self::Once { at_unix_ms } => {
                if *at_unix_ms <= after_unix_ms {
                    Ok(None)
                } else {
                    Ok(Some(ScheduledInstant {
                        unix_ms: *at_unix_ms,
                        local_rfc3339: utc_rfc3339(*at_unix_ms)?,
                    }))
                }
            }
            Self::Interval {
                anchor_unix_ms,
                every_ms,
            } => {
                if *every_ms == 0 {
                    return Err(AutomationError::new(
                        AutomationErrorKind::InvalidInput,
                        "schedule.interval.next",
                        false,
                        "The schedule interval must be non-zero",
                    ));
                }
                let next = if after_unix_ms < *anchor_unix_ms {
                    *anchor_unix_ms
                } else {
                    let elapsed = after_unix_ms.saturating_sub(*anchor_unix_ms);
                    let periods = elapsed / *every_ms + 1;
                    anchor_unix_ms.saturating_add(periods.saturating_mul(*every_ms))
                };
                Ok(Some(ScheduledInstant {
                    unix_ms: next,
                    local_rfc3339: utc_rfc3339(next)?,
                }))
            }
            Self::Cron(schedule) => schedule.next_after(after_unix_ms).map(Some),
        }
    }

    /// Applies startup catch-up without replaying every missed occurrence.
    ///
    /// # Errors
    ///
    /// Returns an error when the plan cannot calculate its next occurrence.
    pub fn catch_up(
        &self,
        last_observed_unix_ms: u64,
        now_unix_ms: u64,
        policy: MissedRunPolicy,
    ) -> AutomationResult<CatchUpDecision> {
        let first = self.next_after(last_observed_unix_ms)?;
        let missed = first
            .as_ref()
            .filter(|instant| instant.unix_ms <= now_unix_ms)
            .cloned();
        let next = self.next_after(now_unix_ms)?;
        Ok(CatchUpDecision {
            fire_now: missed.is_some() && policy == MissedRunPolicy::RunOnce,
            missed,
            next,
        })
    }
}

/// Startup result for a persisted declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatchUpDecision {
    pub fire_now: bool,
    pub missed: Option<ScheduledInstant>,
    pub next: Option<ScheduledInstant>,
}

fn utc_from_millis(unix_ms: u64) -> AutomationResult<DateTime<Utc>> {
    let unix_ms = i64::try_from(unix_ms).map_err(|_| invalid_timestamp())?;
    Utc.timestamp_millis_opt(unix_ms)
        .single()
        .ok_or_else(invalid_timestamp)
}

fn utc_rfc3339(unix_ms: u64) -> AutomationResult<String> {
    utc_from_millis(unix_ms).map(|value| value.to_rfc3339())
}

fn invalid_timestamp() -> AutomationError {
    AutomationError::new(
        AutomationErrorKind::InvalidInput,
        "schedule.timestamp",
        false,
        "The schedule timestamp is outside the supported range",
    )
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone as _, Utc};
    use chrono_tz::America::New_York;

    use super::{CronSchedule, MissedRunPolicy, SchedulePlan};

    fn millis(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> u64 {
        u64::try_from(
            Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
                .single()
                .expect("valid UTC instant")
                .timestamp_millis(),
        )
        .expect("positive timestamp")
    }

    #[test]
    fn interval_is_anchored_and_catch_up_runs_at_most_once() {
        let plan = SchedulePlan::Interval {
            anchor_unix_ms: 100,
            every_ms: 10,
        };
        assert_eq!(
            plan.next_after(105)
                .expect("next")
                .expect("instant")
                .unix_ms,
            110
        );
        let run_once = plan
            .catch_up(100, 135, MissedRunPolicy::RunOnce)
            .expect("catch up");
        assert!(run_once.fire_now);
        assert_eq!(run_once.missed.expect("missed").unix_ms, 110);
        assert_eq!(run_once.next.expect("next").unix_ms, 140);
        assert!(
            !plan
                .catch_up(100, 135, MissedRunPolicy::Skip)
                .expect("skip")
                .fire_now
        );
    }

    #[test]
    fn cron_uses_explicit_timezone_and_skips_nonexistent_dst_time() {
        let schedule =
            CronSchedule::parse("0 30 2 * * * *", "America/New_York").expect("cron schedule");
        let before = New_York
            .with_ymd_and_hms(2026, 3, 8, 0, 0, 0)
            .single()
            .expect("before DST gap")
            .with_timezone(&Utc);
        let next = schedule
            .next_after(u64::try_from(before.timestamp_millis()).expect("positive"))
            .expect("next occurrence");
        assert!(next.local_rfc3339.starts_with("2026-03-09T02:30:00"));
    }

    #[test]
    fn one_shot_expires_after_its_instant() {
        let at = millis(2026, 7, 28, 1, 0);
        let plan = SchedulePlan::Once { at_unix_ms: at };
        assert!(plan.next_after(at - 1).expect("next").is_some());
        assert!(plan.next_after(at).expect("expired").is_none());
    }
}
