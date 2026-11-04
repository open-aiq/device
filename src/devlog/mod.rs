//! Important device events captured for upload to `POST /device/logs`.
//!
//! A `log::Log` wrapper copies every WARN/ERROR record, plus INFO records logged
//! through [`event!`](crate::event) (target `event::<tag>`), into a bounded ring
//! buffer. Everything is still printed to the serial console by the inner logger.
//! Never log secrets (WiFi password, device key): they would be uploaded.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::Serialize;

pub const BUFFER_CAPACITY: usize = 64;
pub const BATCH_LIMIT: usize = 50;
pub const MESSAGE_LIMIT: usize = 200;
const TAG_LIMIT: usize = 32;
const EVENT_TARGET_PREFIX: &str = "event::";
/// Unix timestamps before this are treated as "clock not synced yet".
const MIN_VALID_UNIX: i64 = 1_700_000_000;

/// Logs an important INFO event that is also uploaded to the backend.
///
/// `event!("wifi", "connected ip={}", ip);`
#[macro_export]
macro_rules! event {
    ($tag:literal, $($arg:tt)+) => {
        ::log::info!(target: concat!("event::", $tag), $($arg)+)
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LogEntry {
    pub level: Level,
    pub tag: String,
    pub message: String,
    pub uptime_ms: u64,
    pub boot_count: u32,
    /// Unix seconds once SNTP has synced, otherwise `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<i64>,
}

#[derive(Debug, Default)]
pub struct LogBuffer {
    entries: VecDeque<LogEntry>,
    dropped: u32,
}

impl LogBuffer {
    pub fn push(&mut self, mut entry: LogEntry) {
        truncate(&mut entry.message, MESSAGE_LIMIT);
        truncate(&mut entry.tag, TAG_LIMIT);
        if self.entries.len() >= BUFFER_CAPACITY {
            self.entries.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.entries.push_back(entry);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn dropped(&self) -> u32 {
        self.dropped
    }

    /// The oldest entries (at most [`BATCH_LIMIT`]) and the dropped counter, as an upload payload.
    pub fn batch_payload(&self) -> Option<(usize, String)> {
        if self.entries.is_empty() && self.dropped == 0 {
            return None;
        }
        let count = self.entries.len().min(BATCH_LIMIT);
        #[derive(Serialize)]
        struct Payload<'a> {
            entries: Vec<&'a LogEntry>,
            dropped: u32,
        }
        let payload = Payload {
            entries: self.entries.iter().take(count).collect(),
            dropped: self.dropped,
        };
        serde_json::to_string(&payload)
            .ok()
            .map(|json| (count, json))
    }

    /// Removes `count` uploaded entries and resets the dropped counter.
    pub fn acknowledge(&mut self, count: usize) {
        let count = count.min(self.entries.len());
        self.entries.drain(..count);
        self.dropped = 0;
    }
}

fn truncate(value: &mut String, limit: usize) {
    if value.len() > limit {
        let mut end = limit;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
    }
}

pub static BUFFER: Mutex<LogBuffer> = Mutex::new(LogBuffer {
    entries: VecDeque::new(),
    dropped: 0,
});

/// Decides whether a record is captured, and with which tag.
pub fn classify(level: log::Level, target: &str) -> Option<(Level, String)> {
    if let Some(tag) = target.strip_prefix(EVENT_TARGET_PREFIX) {
        let level = match level {
            log::Level::Error => Level::Error,
            log::Level::Warn => Level::Warn,
            _ => Level::Info,
        };
        return Some((level, tag.to_owned()));
    }
    let level = match level {
        log::Level::Error => Level::Error,
        log::Level::Warn => Level::Warn,
        _ => return None,
    };
    let tag = target.rsplit("::").next().unwrap_or(target);
    Some((level, tag.to_owned()))
}

pub fn unix_now() -> Option<i64> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    (secs >= MIN_VALID_UNIX).then_some(secs)
}

/// Wraps the console logger and captures important records into [`BUFFER`].
pub struct CaptureLogger {
    pub inner: &'static (dyn log::Log + Sync),
    pub uptime_ms: fn() -> u64,
    pub boot_count: std::sync::atomic::AtomicU32,
}

impl log::Log for CaptureLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        self.inner.log(record);
        if let Some((level, tag)) = classify(record.level(), record.target()) {
            let entry = LogEntry {
                level,
                tag,
                message: record.args().to_string(),
                uptime_ms: (self.uptime_ms)(),
                boot_count: self.boot_count.load(std::sync::atomic::Ordering::Relaxed),
                ts: unix_now(),
            };
            // try_lock: never block or deadlock inside the logger.
            if let Ok(mut buffer) = BUFFER.try_lock() {
                buffer.push(entry);
            }
        }
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

#[cfg(target_os = "espidf")]
pub mod esp;

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(message: &str) -> LogEntry {
        LogEntry {
            level: Level::Warn,
            tag: "wifi".into(),
            message: message.into(),
            uptime_ms: 10,
            boot_count: 2,
            ts: None,
        }
    }

    #[test]
    fn classifies_events_and_warnings_only() {
        assert_eq!(
            classify(log::Level::Info, "event::wifi"),
            Some((Level::Info, "wifi".into()))
        );
        assert_eq!(
            classify(log::Level::Warn, "device_rust::telemetry::esp"),
            Some((Level::Warn, "esp".into()))
        );
        assert_eq!(
            classify(log::Level::Error, "main"),
            Some((Level::Error, "main".into()))
        );
        assert_eq!(classify(log::Level::Info, "device_rust"), None);
        assert_eq!(classify(log::Level::Debug, "x"), None);
    }

    #[test]
    fn ring_buffer_drops_oldest_and_counts() {
        let mut buffer = LogBuffer::default();
        for i in 0..BUFFER_CAPACITY + 3 {
            buffer.push(entry(&format!("m{i}")));
        }
        assert_eq!(buffer.len(), BUFFER_CAPACITY);
        assert_eq!(buffer.dropped(), 3);
        let (count, json) = buffer.batch_payload().unwrap();
        assert_eq!(count, BATCH_LIMIT);
        assert!(json.contains("\"dropped\":3"));
        assert!(
            json.contains("\"message\":\"m3\""),
            "oldest kept entry first"
        );
        assert!(!json.contains("\"ts\""), "unsynced clock omits ts");
        buffer.acknowledge(count);
        assert_eq!(buffer.len(), BUFFER_CAPACITY - BATCH_LIMIT);
        assert_eq!(buffer.dropped(), 0);
    }

    #[test]
    fn truncates_long_messages_on_char_boundaries() {
        let mut buffer = LogBuffer::default();
        buffer.push(entry(&"é".repeat(150)));
        let (_, json) = buffer.batch_payload().unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let message = value["entries"][0]["message"].as_str().unwrap();
        assert!(message.len() <= MESSAGE_LIMIT);
    }

    #[test]
    fn empty_buffer_has_no_payload() {
        assert!(LogBuffer::default().batch_payload().is_none());
    }
}
