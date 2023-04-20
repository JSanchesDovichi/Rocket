//! Rocket's logging infrastructure.

use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};

use is_terminal::IsTerminal;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
//use yansi::Paint;

/// Reexport the `log` crate as `private`.
#[cfg(not(feature = "tracing-logger"))]
pub use log as private;
use private::span;

#[cfg(feature = "tracing-logger")]
pub use tracing as private;

use yansi::Paint;

use crate::{config::LogLevel, log_utils::RocketLogger};

// Expose logging macros (hidden) for use by core/contrib codegen.
macro_rules! define_log_macro {
    ($name:ident: $kind:ident, $target:expr, $d:tt) => (
        #[doc(hidden)]
        #[macro_export]
        macro_rules! $name {
            ($d ($t:tt)*) => ($crate::log::private::$kind!(target: $target, $d ($t)*))
        }
    );
    ($name:ident ($indented:ident): $kind:ident, $target:expr, $d:tt) => (
        define_log_macro!($name: $kind, $target, $d);
        define_log_macro!($indented: $kind, concat!($target, "::_"), $d);
    );
    ($kind:ident, $indented:ident) => (
        define_log_macro!($kind: $kind, module_path!(), $);
        define_log_macro!($indented: $kind, concat!(module_path!(), "::_"), $);

        pub use $indented;
    );
}

define_log_macro!(error, error_);
define_log_macro!(warn, warn_);
define_log_macro!(info, info_);
define_log_macro!(debug, debug_);
define_log_macro!(trace, trace_);
define_log_macro!(launch_meta (launch_meta_): info, "rocket::launch", $);
define_log_macro!(launch_info (launch_msg_): warn, "rocket::launch", $);

// `print!` panics when stdout isn't available, but this macro doesn't. See
// SergioBenitez/Rocket#2019 and rust-lang/rust#46016 for more.
//
// Unfortunately, `libtest` captures output by replacing a special sink that
// `print!`, and _only_ `print!`, writes to. Using `write!` directly bypasses
// this sink. As a result, using this better implementation for logging means
// that test log output isn't captured, muddying `cargo test` output.
//
// As a compromise, we only use this better implementation when we're not
// compiled with `debug_assertions` or running tests, so at least tests run in
// debug-mode won't spew output. NOTE: `cfg(test)` alone isn't sufficient: the
// crate is compiled normally for integration tests.
#[cfg(not(any(debug_assertions, test, doctest)))]
macro_rules! write_out {
    ($($arg:tt)*) => ({
        use std::io::{Write, stdout, stderr};
        let _ = write!(stdout(), $($arg)*).or_else(|e| write!(stderr(), "{}", e));
    })
}

#[cfg(not(feature = "tracing-logger"))]
#[cfg(any(debug_assertions, test, doctest))]
macro_rules! write_out {
    ($($arg:tt)*) => (print!($($arg)*))
}

// Whether a record is a special `launch_{meta,info}!` record.
#[cfg(not(feature = "tracing-logger"))]
fn is_launch_record(record: &log::Metadata<'_>) -> bool {
    record.target().contains("rocket::launch")
}

#[cfg(not(feature = "tracing-logger"))]
impl log::Log for RocketLogger {
    #[inline(always)]
    fn enabled(&self, record: &log::Metadata<'_>) -> bool {
        match log::max_level().to_level() {
            Some(max) => record.level() <= max || is_launch_record(record),
            None => false,
        }
    }

    fn log(&self, record: &log::Record<'_>) {
        // Print nothing if this level isn't enabled and this isn't launch info.
        if !self.enabled(record.metadata()) {
            return;
        }

        // Don't print Hyper, Rustls or r2d2 messages unless debug is enabled.
        let max = log::max_level();
        let from = |path| record.module_path().map_or(false, |m| m.starts_with(path));
        let debug_only = from("hyper") || from("rustls") || from("r2d2");
        if log::LevelFilter::from(LogLevel::Debug) > max && debug_only {
            return;
        }

        // In Rocket, we abuse targets with suffix "_" to indicate indentation.
        let indented = record.target().ends_with('_');
        if indented {
            write_out!("   {} ", Paint::default(">>").bold());
        }

        // Downgrade a physical launch `warn` to logical `info`.
        let level = is_launch_record(record.metadata())
            .then(|| log::Level::Info)
            .unwrap_or_else(|| record.level());

        match level {
            log::Level::Error if !indented => {
                write_out!(
                    "{} {}\n",
                    Paint::red("Error:").bold(),
                    Paint::red(record.args()).wrap()
                );
            }
            log::Level::Warn if !indented => {
                write_out!(
                    "{} {}\n",
                    Paint::yellow("Warning:").bold(),
                    Paint::yellow(record.args()).wrap()
                );
            }
            log::Level::Info => write_out!("{}\n", Paint::blue(record.args()).wrap()),
            log::Level::Trace => write_out!("{}\n", Paint::magenta(record.args()).wrap()),
            log::Level::Warn => write_out!("{}\n", Paint::yellow(record.args()).wrap()),
            log::Level::Error => write_out!("{}\n", Paint::red(record.args()).wrap()),
            log::Level::Debug => {
                write_out!("\n{} ", Paint::blue("-->").bold());
                if let Some(file) = record.file() {
                    write_out!("{}", Paint::blue(file));
                }

                if let Some(line) = record.line() {
                    write_out!(":{}\n", Paint::blue(line));
                }

                write_out!("\t{}\n", record.args());
            }
        }
    }

    fn flush(&self) {
        // NOOP: We don't buffer any records.
    }
}

pub(crate) fn init_default() {
    crate::log::init(&crate::Config::debug_default())
}

#[cfg(not(feature = "tracing-logger"))]
pub(crate) fn init(config: &crate::Config) {
    static ROCKET_LOGGER_SET: AtomicBool = AtomicBool::new(false);

    // Try to initialize Rocket's logger, recording if we succeeded.
    if log::set_boxed_logger(Box::new(RocketLogger)).is_ok() {
        ROCKET_LOGGER_SET.store(true, Ordering::Release);
    }

    // Always disable colors if requested or if they won't work on Windows.
    if !config.cli_colors || !Paint::enable_windows_ascii() {
        Paint::disable();
    }

    // Set Rocket-logger specific settings only if Rocket's logger is set.
    if ROCKET_LOGGER_SET.load(Ordering::Acquire) {
        // Rocket logs to stdout, so disable coloring if it's not a TTY.
        if !std::io::stdout().is_terminal() {
            Paint::disable();
        }

        log::set_max_level(config.log_level.into());
    }
}

#[cfg(feature = "tracing-logger")]
pub(crate) fn init(config: &crate::Config) {
    use tracing::subscriber::set_global_default;
    use tracing_subscriber::FmtSubscriber;

    let my_subscriber = FmtSubscriber::builder()
        .without_time()
        .with_level(false)
        .with_file(false)
        .with_line_number(false)
        .with_target(false)
        .with_max_level(config.log_level)
        .finish();

        if let Err(e) = set_global_default(my_subscriber) {
            tracing::warn!("Global subscriber already set: {e}");
        }
}