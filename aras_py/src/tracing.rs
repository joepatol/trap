use std::fmt;
use std::process;

use log::warn;
use tracing::Level;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::time::{FormatTime, SystemTime};
use tracing_subscriber::registry::LookupSpan;

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const BOLD_CYAN: &str = "\x1b[1;36m";

pub(crate) fn init(log_level: &str, worker_mode: bool) {
    let formatter = if worker_mode {
        TracingFormatter::new_worker(process::id())
    } else {
        TracingFormatter::new()
    };

    tracing_subscriber::fmt()
        .with_max_level(get_log_level_filter(log_level))
        .event_format(formatter)
        .init();
}

struct TracingFormatter {
    pid: Option<u32>,
}

impl TracingFormatter {
    fn new_worker(pid: u32) -> Self {
        Self { pid: Some(pid) }
    }

    fn new() -> Self {
        Self { pid: None }
    }
}

impl<S, N> FormatEvent<S, N> for TracingFormatter
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &tracing_subscriber::fmt::FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> fmt::Result {
        let ansi = writer.has_ansi_escapes();

        // Timestamp (dimmed)
        if ansi { write!(writer, "{}", DIM)?; }
        SystemTime.format_time(&mut writer)?;
        if ansi { write!(writer, "{}", RESET)?; }
        write!(writer, " ")?;
        
        if self.pid.is_some() {
            // worker [pid] (bold cyan)
            if ansi { write!(writer, "{}", BOLD_CYAN)?; }
            write!(writer, "worker [{}]", self.pid.unwrap())?;
            if ansi { write!(writer, "{}", RESET)?; }
            write!(writer, "")?;
        }

        // Level (colored)
        let level = *event.metadata().level();
        if ansi { write!(writer, "{}", level_color(&level))?; }
        write!(writer, "{:>5}", level)?;
        if ansi { write!(writer, "{}", RESET)?; }
        write!(writer, " ")?;

        // Message / fields
        ctx.format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

fn get_log_level_filter(log_level: &str) -> tracing::Level {
    match log_level {
        "DEBUG" => tracing::Level::DEBUG,
        "INFO" => tracing::Level::INFO,
        "ERROR" => tracing::Level::ERROR,
        "OFF" => tracing::Level::ERROR,
        "TRACE" => tracing::Level::TRACE,
        "WARN" => tracing::Level::WARN,
        _ => {
            warn!("Invalid log level '{}'; defaulting to INFO", log_level);
            tracing::Level::INFO
        }
    }
}

fn level_color(level: &Level) -> &'static str {
    match *level {
        Level::ERROR => "\x1b[1;31m",
        Level::WARN => "\x1b[1;33m",
        Level::INFO => "\x1b[1;32m",
        Level::DEBUG => "\x1b[1;34m",
        Level::TRACE => "\x1b[1;35m",
    }
}