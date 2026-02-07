use std::error::Error;

use crate::WithBacktrace;

pub type AnyError = Box<dyn Error + Send + Sync>;

pub type ResultBt<T, E> = Result<T, WithBacktrace<E>>;

pub type ResultBtAny<T> = Result<T, WithBacktrace<AnyError>>;

define_to_dyn!(&str);
define_to_dyn!(String);

define_to_dyn!(std::io::Error);

define_to_dyn!(std::num::TryFromIntError);

define_to_dyn!(serde_json::Error);

define_to_dyn!(sqlx::Error);
define_to_dyn!(sqlx::migrate::MigrateError);

#[cfg(target_os = "windows")]
define_to_dyn!(windows_service::Error);

#[cfg(target_os = "windows")]
define_to_dyn!(windows_result::Error);

#[macro_export]
macro_rules! unwrap_or {
    ($to_unwrap: expr, $e: ident, $else_do: expr) => {{
        match $to_unwrap {
            Ok(x) => x,
            Err($e) => _ = $else_do,
        }
    }};
    ($to_unwrap: expr, $else_do: expr) => {{
        match $to_unwrap {
            Some(x) => x,
            None => _ = $else_do,
        }
    }};
}
