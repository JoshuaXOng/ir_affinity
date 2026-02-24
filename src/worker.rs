use sqlx::SqlitePool;
use sysinfo::{Process, System};
use tokio::{sync::watch, task::JoinHandle};
use tracing::info;

use crate::{
    errors::ResultBtAny,
    persistence::{CpuSelections, PersistentStore},
    selections::mask_to_hashset,
    unwrap_or,
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
    is_synced: Option<bool>,
    error: Option<String>,
}

impl WorkerHeartbeat {
    pub fn now(is_simulation_synced: Option<bool>, error: Option<String>) -> Self {
        Self {
            at: chrono::Utc::now(),
            is_synced: is_simulation_synced,
            error,
        }
    }

    pub fn get_at(&self) -> &chrono::DateTime<chrono::Utc> {
        &self.at
    }

    pub fn get_is_synced(&self) -> &Option<bool> {
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

pub fn spawn_worker_task(
    sqlite_pool: SqlitePool,
    worker_status: watch::Sender<Option<WorkerHeartbeat>>,
) -> JoinHandle<ResultBtAny<()>> {
    tokio::task::spawn(async move {
        let mut system_info = System::new();
        system_info.refresh_all();
        info!("Refreshing system info.");

        let worker_period = std::time::Duration::from_secs(WORKER_COOLDOWN_PERIOD_SECONDS);
        loop {
            tokio::time::sleep(worker_period).await;

            let persistent_store = unwrap_or!(
                PersistentStore::load(system_info.cpus().len(), &sqlite_pool).await,
                e,
                {
                    worker_status
                        .send_replace(Some(WorkerHeartbeat::now(None, Some(e.get().to_string()))));
                    continue;
                }
            );

            let iracing_simulators: Vec<&Process> = system_info
                .processes_by_exact_name(persistent_store.process.as_ref())
                .collect();
            let are_no_simulators = iracing_simulators.is_empty();
            if are_no_simulators {
                worker_status.send_replace(Some(WorkerHeartbeat::now(None, None)));
            } else {
                let mut are_synced = unwrap_or!(
                    get_are_simulators_affinity_synced(&persistent_store, &system_info).await,
                    e,
                    {
                        worker_status.send_replace(Some(WorkerHeartbeat::now(
                            None,
                            Some(e.get().to_string()),
                        )));
                        continue;
                    }
                );

                if !are_synced {
                    unwrap_or!(
                        sync_simulators_affinity(&persistent_store, &system_info).await,
                        e,
                        {
                            worker_status.send_replace(Some(WorkerHeartbeat::now(
                                Some(are_synced),
                                Some(e.get().to_string()),
                            )));
                            continue;
                        }
                    );
                }

                are_synced = unwrap_or!(
                    get_are_simulators_affinity_synced(&persistent_store, &system_info).await,
                    e,
                    {
                        worker_status.send_replace(Some(WorkerHeartbeat::now(
                            Some(are_synced),
                            Some(e.get().to_string()),
                        )));
                        continue;
                    }
                );

                worker_status.send_replace(Some(WorkerHeartbeat::now(Some(are_synced), None)));
            }
        }
    })
}

async fn get_are_simulators_affinity_synced(
    persistent_store: &PersistentStore,
    system_info: &System,
) -> ResultBtAny<bool> {
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

fn get_cpu_affinity_of_process(#[allow(unused_variables)] process: &Process) -> ResultBtAny<usize> {
    #[cfg(target_os = "windows")]
    unsafe {
        let should_inherit_handle = false;
        let process = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION,
            should_inherit_handle,
            process.pid().as_u32(),
        )?;
        info!("Opened process.");

        let mut process_mask: usize = 0;
        let mut system_mask: usize = 0;
        let is_success = GetProcessAffinityMask(
            process,
            &mut process_mask as *mut usize,
            &mut system_mask as *mut usize,
        );
        CloseHandle(process)?;
        info!("Closed process.");
        is_success?;
        info!("Got affinity mask.");

        Ok(process_mask)
    }

    #[cfg(target_os = "linux")]
    unimplemented!()
}

async fn sync_simulators_affinity(
    persistent_store: &PersistentStore,
    system_info: &System,
) -> ResultBtAny<()> {
    let iracing_simulators: Vec<&Process> = system_info
        .processes_by_exact_name(persistent_store.process.as_ref())
        .collect();

    for iracing_simulator in iracing_simulators {
        set_cpu_affinity_of_process(iracing_simulator, persistent_store).await?;
    }

    Ok(())
}

async fn set_cpu_affinity_of_process(
    #[allow(unused_variables)] process: &Process,
    persistent_store: &PersistentStore,
) -> ResultBtAny<()> {
    #[allow(unused_variables)]
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
            info!("Closed handle.");
            is_set?;
            info!("Set CPU affinity.");
        }
    }

    Ok(())
}
