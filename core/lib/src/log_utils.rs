use yansi::Paint;
use std::str::FromStr;
use std::fmt;
use serde::{de, Serialize, Serializer, Deserialize, Deserializer};

#[derive(Debug)]
pub struct RocketLogger;

pub trait PaintExt {
    fn emoji(item: &str) -> Paint<&str>;
}

impl PaintExt for Paint<&str> {
    /// Paint::masked(), but hidden on Windows due to broken output. See #1122.
    fn emoji(_item: &str) -> Paint<&str> {
        #[cfg(windows)] { Paint::masked("") }
        #[cfg(not(windows))] { Paint::masked(_item) }
    }
}

/// Defines the maximum level of log messages to show.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum LogLevel {
    /// Only shows errors and warnings: `"critical"`.
    Critical,
    /// Shows everything except debug and trace information: `"normal"`.
    Normal,
    /// Shows everything: `"debug"`.
    Debug,
    /// Shows nothing: "`"off"`".
    Off,
}

impl From<LogLevel> for log::LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Critical => log::LevelFilter::Warn,
            LogLevel::Normal => log::LevelFilter::Info,
            LogLevel::Debug => log::LevelFilter::Trace,
            LogLevel::Off => log::LevelFilter::Off
        }
    }
}

impl From<LogLevel> for tracing::level_filters::LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Critical => tracing::level_filters::LevelFilter::WARN,
            LogLevel::Normal => tracing::level_filters::LevelFilter::INFO,
            LogLevel::Debug => tracing::level_filters::LevelFilter::TRACE,
            LogLevel::Off => tracing::level_filters::LevelFilter::OFF
        }
    }
}

impl LogLevel {
    fn as_str(&self) -> &str {
        match self {
            LogLevel::Critical => "critical",
            LogLevel::Normal => "normal",
            LogLevel::Debug => "debug",
            LogLevel::Off => "off",
        }
    }
}

impl FromStr for LogLevel {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let level = match &*s.to_ascii_lowercase() {
            "critical" => LogLevel::Critical,
            "normal" => LogLevel::Normal,
            "debug" => LogLevel::Debug,
            "off" => LogLevel::Off,
            _ => return Err("a log level (off, debug, normal, critical)")
        };

        Ok(level)
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for LogLevel {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LogLevel {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let string = String::deserialize(de)?;
        LogLevel::from_str(&string).map_err(|_| de::Error::invalid_value(
            de::Unexpected::Str(&string),
            &figment::error::OneOf( &["critical", "normal", "debug", "off"])
        ))
    }
}