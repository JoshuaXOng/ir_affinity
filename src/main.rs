#![cfg_attr(test, allow(unused))]

use bt_error::define_with_backtrace;
use tokio::sync::watch;
use tracing::error;

use crate::{
    errors::ResultBtAny, persistence::PersistentStore, ui::run_ui, worker::spawn_worker_task,
};

define_with_backtrace!();

pub mod errors;
pub mod ir;
pub mod persistence;
pub mod selections;
pub mod ui;
pub mod worker;

fn main() {
    if let Err(e) = main_() {
        error!("{:?}", e);
        std::process::exit(1);
    } else {
        std::process::exit(0);
    }
}

fn main_() -> ResultBtAny<()> {
    tracing_subscriber::fmt::init();

    let (status_sender, status_receiver) = watch::channel(None);

    let other_runtime = tokio::runtime::Runtime::new()?;

    let sqlite_pool = other_runtime.block_on(PersistentStore::create_pool())?;

    let sqlite_pool_2 = sqlite_pool.clone();
    let sqlite_pool_3 = sqlite_pool.clone();

    let persistent_store = other_runtime.block_on(async {
        let mut system_info = sysinfo::System::new();
        system_info.refresh_all();
        PersistentStore::load(system_info.cpus().len(), &sqlite_pool).await
    })?;

    other_runtime.spawn_blocking(|| {
        spawn_worker_task(sqlite_pool_2, status_sender);
    });

    run_ui(persistent_store, sqlite_pool_3, status_receiver)?;

    Ok(())
}
