#![cfg_attr(test, allow(unused))]

use bt_error::define_with_backtrace;

define_with_backtrace!();

pub mod selections;
pub mod persistence;
pub mod errors;
pub mod ir;
pub mod worker;
