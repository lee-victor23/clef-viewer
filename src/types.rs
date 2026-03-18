use chrono::{DateTime, Local, NaiveDate, Timelike, Utc};
use serde_json::Value;

// ── Level ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Copy, Hash)]
pub enum Level {
    Verbose = 0,
    Debug   = 1,
    Info    = 2,
    Warning = 3,
    Error   = 4,
    Fatal   = 5,
    Unknown = 6,
}

impl Level {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "verbose" | "v"                => Level::Verbose,
            "debug"   | "d"                => Level::Debug,
            "information" | "info" | "i"   => Level::Info,
            "warning" | "warn" | "w"       => Level::Warning,
            "error"   | "e"                => Level::Error,
            "fatal"   | "f"                => Level::Fatal,
            _                              => Level::Unknown,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Level::Verbose => "Verbose",
            Level::Debug   => "Debug",
            Level::Info    => "Information",
            Level::Warning => "Warning",
            Level::Error   => "Error",
            Level::Fatal   => "Fatal",
            Level::Unknown => "Unknown",
        }
    }

    pub fn short(&self) -> &'static str {
        match self {
            Level::Verbose => "VRB",
            Level::Debug   => "DBG",
            Level::Info    => "INF",
            Level::Warning => "WRN",
            Level::Error   => "ERR",
            Level::Fatal   => "FTL",
            Level::Unknown => "???",
        }
    }

}

pub const ALL_LEVELS: [Level; 6] = [
    Level::Verbose, Level::Debug, Level::Info,
    Level::Warning, Level::Error, Level::Fatal,
];

// ── DateFilter ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct DateFilter {
    pub date:    NaiveDate,
    pub hour:    u32,
    pub minute:  u32,
    pub second:  u32,
    pub enabled: bool,
}

impl DateFilter {
    pub fn empty() -> Self {
        let n = Local::now();
        Self { date: n.date_naive(), hour: 0, minute: 0, second: 0, enabled: false }
    }

    pub fn from_local_dt(dt: DateTime<Local>) -> Self {
        Self { date: dt.date_naive(), hour: dt.hour(), minute: dt.minute(), second: dt.second(), enabled: false }
    }

    pub fn to_utc(&self) -> Option<DateTime<Utc>> {
        if !self.enabled { return None; }
        self.date.and_hms_opt(self.hour, self.minute, self.second)
            .and_then(|ndt| ndt.and_local_timezone(Local).single())
            .map(|l| l.with_timezone(&Utc))
    }
}

// ── LogRecord ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct LogRecord {
    pub timestamp_utc:   String,
    pub dt_utc:          Option<DateTime<Utc>>,
    pub timestamp_local: String,
    pub level:           Level,
    pub message:         String,
    pub template:        String,
    pub exception:       String,
    pub raw:             Value,
    pub line_no:         usize,
}

// ── LevelStats ────────────────────────────────────────────────────────────────

pub struct LevelStats { pub counts: [usize; 7] }

impl LevelStats {
    pub fn from_filtered(records: &[LogRecord], filtered: &[usize]) -> Self {
        let mut counts = [0usize; 7];
        for &i in filtered { counts[records[i].level as usize] += 1; }
        LevelStats { counts }
    }
    pub fn total(&self) -> usize { self.counts.iter().sum() }
    pub fn count(&self, l: Level) -> usize { self.counts[l as usize] }
}

// ── TemplateSummary ───────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct TemplateSummary {
    pub template: String,
    pub count:    usize,
    pub level:    Level,
}

// ── Tab ───────────────────────────────────────────────────────────────────────

#[derive(PartialEq)]
pub enum Tab { Logs, Templates }

// ── LoadState ────────────────────────────────────────────────────────────────

pub enum LoadState {
    Idle,
    Loading,
}
