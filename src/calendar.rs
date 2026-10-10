use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveTime, TimeZone};

#[derive(Debug, Clone)]
pub struct Calendar {
    pub selected: NaiveDate,
}

impl Default for Calendar {
    fn default() -> Self {
        Self {
            selected: Local::now().date_naive(),
        }
    }
}

impl Calendar {
    pub fn shift_days(&mut self, days: i64) {
        if let Some(next) = self
            .selected
            .checked_add_signed(chrono::Duration::days(days))
        {
            self.selected = next;
        }
    }

    pub fn shift_month(&mut self, delta: i32) {
        let month = self.selected.year() * 12 + self.selected.month0() as i32 + delta;
        let year = month.div_euclid(12);
        let month = month.rem_euclid(12) as u32 + 1;
        if let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) {
            self.selected = first
                .with_day(self.selected.day().min(days_in_month(year, month)))
                .unwrap_or(first);
        }
    }

    /// Monday-first month grid, padded to six weeks so dialogs do not jump.
    pub fn cells(&self) -> Vec<Option<NaiveDate>> {
        let first = self.selected.with_day(1).unwrap();
        let padding = first.weekday().num_days_from_monday() as usize;
        (0..42)
            .map(|cell| {
                if cell < padding {
                    None
                } else {
                    first.with_day((cell - padding + 1) as u32)
                }
            })
            .collect()
    }

    pub fn jira_started(&self, time: &str) -> Result<String, String> {
        let time = NaiveTime::parse_from_str(time.trim(), "%H:%M")
            .map_err(|_| "Start time must be HH:MM (24-hour clock)".to_string())?;
        let local = Local.from_local_datetime(&self.selected.and_time(time)).single()
            .ok_or("This local time is ambiguous or invalid due to daylight saving; choose another time")?;
        Ok(local.format("%Y-%m-%dT%H:%M:%S.000%z").to_string())
    }
}

pub fn parse_started(value: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f%z")
        .ok()
        .or_else(|| DateTime::parse_from_rfc3339(value).ok())
        .map(|t| t.with_timezone(&Local))
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    (28..=31)
        .rev()
        .find(|day| NaiveDate::from_ymd_opt(year, month, *day).is_some())
        .unwrap_or(0)
}

pub fn duration_text(seconds: i64) -> String {
    format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn month_navigation_clamps_and_wraps() {
        let mut calendar = Calendar {
            selected: NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
        };
        calendar.shift_month(1);
        assert_eq!(calendar.selected.to_string(), "2024-02-29");
        calendar.shift_month(-2);
        assert_eq!(calendar.selected.to_string(), "2023-12-29");
        assert_eq!(days_in_month(2100, 2), 28);
    }
    #[test]
    fn calendar_is_monday_first_and_contains_all_days() {
        let calendar = Calendar {
            selected: NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
        };
        let cells = calendar.cells();
        assert_eq!(cells.len(), 42);
        assert!(cells[..6].iter().all(Option::is_none));
        assert_eq!(cells.iter().flatten().count(), 31);
    }
    #[test]
    fn timestamps_preserve_offset_and_validate_input() {
        let calendar = Calendar {
            selected: NaiveDate::from_ymd_opt(2026, 7, 26).unwrap(),
        };
        let started = calendar.jira_started("09:30").unwrap();
        assert!(started.starts_with("2026-07-26T09:30:00.000"));
        assert_eq!(
            parse_started(&started).unwrap().date_naive(),
            calendar.selected
        );
        assert!(calendar.jira_started("25:00").is_err());
    }
}
