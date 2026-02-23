use sqlx::SqlitePool;
use sysinfo::{Process, System};
use tokio::sync::watch;
use tracing::info;

use crate::{
    errors::ResultBtAny,
    persistence::{CpuSelections, PersistentStore},
    selections::mask_to_hashset,
};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::CloseHandle;
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::{
    GetProcessAffinityMask, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SET_INFORMATION, SetProcessAffinityMask,
};

const HEARTBEAT_STALE_PERIOD_SECONDS: i64 = 10;

#[derive(Debug, Clone)]
pub struct WorkerHeartbeat {
    at: chrono::DateTime<chrono::Utc>,
    is_synced: bool,
    error: Option<String>,
}

impl WorkerHeartbeat {
    pub fn now(is_simulation_synced: bool, error: Option<String>) -> Self {
        Self {
            at: chrono::Utc::now(),
            is_synced: is_simulation_synced,
            error: None,
        }
    }

    pub fn get_at(&self) -> &chrono::DateTime<chrono::Utc> {
        &self.at
    }

    pub fn get_is_synced(&self) -> &bool {
        &self.is_synced
    }

    pub fn get_error(&self) -> &Option<String> {
        &self.error
    }

    pub fn get_is_stale(&self) -> bool {
        chrono::Utc::now() - self.at > chrono::Duration::seconds(HEARTBEAT_STALE_PERIOD_SECONDS)
    }
}

const WORKER_COOLDOWN_PERIOD_SECONDS: u64 = 5;

// TODO: Deal with unwraps, also display error messages in UI components.
pub fn spawn_worker_task(
    sqlite_pool: SqlitePool,
    worker_status: watch::Sender<Option<WorkerHeartbeat>>,
) {
    tokio::task::spawn(async move {
        let mut system_info = System::new();
        system_info.refresh_all();

        PersistentStore::create_ddl(&sqlite_pool).await.unwrap();

        let worker_period = std::time::Duration::from_secs(WORKER_COOLDOWN_PERIOD_SECONDS);
        loop {
            let mut are_synced = get_are_simulators_affinity_synced(&system_info, &sqlite_pool).await.unwrap();

            if !are_synced {
                sync_simulators_affinity(&system_info, &sqlite_pool).await.unwrap();
            }

            are_synced = get_are_simulators_affinity_synced(&system_info, &sqlite_pool).await.unwrap();

            worker_status.send_replace(Some(WorkerHeartbeat::now(are_synced, None)));

            tokio::time::sleep(worker_period).await;
        }
    });
}

async fn get_are_simulators_affinity_synced(
    system_info: &System,
    sqlite_pool: &SqlitePool,
) -> ResultBtAny<bool> {
    let persistent_store = PersistentStore::load(system_info.cpus().len(), sqlite_pool).await?;

    let iracing_simulators: Vec<&Process> = system_info
        .processes_by_exact_name(persistent_store.process.as_ref())
        .collect();

    for iracing_simulator in iracing_simulators {
        let cpu_affinity = get_cpu_affinity_of_process(iracing_simulator)?;
        let cpu_selections = CpuSelections::new_preselected(
            mask_to_hashset(&cpu_affinity),
            system_info.cpus().len(),
        );
        let isnt_synced = cpu_selections != persistent_store.selections;
        if isnt_synced {
            return Ok(false);
        }
    }

    Ok(true)
}

fn get_cpu_affinity_of_process(process: &Process) -> ResultBtAny<usize> {
    #[cfg(target_os = "windows")]
    unsafe {
        let should_inherit_handle = false;
        let process = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION,
            should_inherit_handle,
            process.pid().as_u32(),
        )?;

        let mut process_mask: usize = 0;
        let mut system_mask: usize = 0;
        let is_success = GetProcessAffinityMask(
            process,
            &mut process_mask as *mut usize,
            &mut system_mask as *mut usize,
        );
        CloseHandle(process)?;
        is_success?;

        Ok(process_mask)
    }

    #[cfg(target_os = "linux")]
    todo!()
}

async fn sync_simulators_affinity(
    system_info: &System,
    sqlite_pool: &SqlitePool,
) -> ResultBtAny<()> {
    let persistent_store = PersistentStore::load(system_info.cpus().len(), sqlite_pool).await?;

    let iracing_simulators: Vec<&Process> = system_info
        .processes_by_exact_name(persistent_store.process.as_ref())
        .collect();

    for iracing_simulator in iracing_simulators {
        set_cpu_affinity_of_process(iracing_simulator, system_info, sqlite_pool).await?;
    }

    Ok(())
}

async fn set_cpu_affinity_of_process(
    process: &Process,
    system_info: &System,
    sqlite_pool: &SqlitePool,
) -> ResultBtAny<()> {
    let persistent_store = PersistentStore::load(system_info.cpus().len(), sqlite_pool).await?;
    let cpu_selections = persistent_store.selections.to_mask();

    #[cfg(target_os = "windows")]
    {
        let process = unsafe {
            let should_inherit_handle = false;
            let process = OpenProcess(
                PROCESS_SET_INFORMATION,
                should_inherit_handle,
                process.pid().as_u32(),
            )?;
            info!("Got process handle.");
            process
        };

        unsafe {
            let is_set = SetProcessAffinityMask(process, cpu_selections);
            CloseHandle(process)?;
            is_set?;
            info!("Set CPU affinity.");
        }
    }

    Ok(())
}
