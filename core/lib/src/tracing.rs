//! Rocket's logging infrastructure.

use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};

use is_terminal::IsTerminal;
use serde::{de, Serialize, Serializer, Deserialize, Deserializer};
use yansi::Paint;

/// Reexport the `log` crate as `private`.
pub use tracing as private;

// Expose logging macros (hidden) for use by core/contrib codegen.
macro_rules! define_log_macro {
    ($name:ident: $kind:ident, $target:expr, $d:tt) => (
        #[doc(hidden)]
        #[macro_export]
        macro_rules! $name {
            ($d ($t:tt)*) => (tracing::$kind!(target: $target, $d ($t)*))
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

pub trait PaintExt {
    fn emoji(item: &str) -> Paint<&str>;
}