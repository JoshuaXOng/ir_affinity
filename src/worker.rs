use std::ffi::OsStr;

use sqlx::SqlitePool;
use sysinfo::{Process, ProcessesToUpdate, System};
use tokio::{sync::watch, task::JoinHandle};
use tracing::{error, info};

#[cfg(target_os = "windows")]
use crate::wrappers::SystemCpuSetInformation;
use crate::{
    errors::ResultBtAny,
    persistence::{CpuSelections, PersistentStore},
    selections::mask_to_hashset,
};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{CloseHandle, GetLastError};
#[cfg(target_os = "windows")]
use windows::Win32::System::SystemInformation::{
    GetSystemCpuSetInformation, SYSTEM_CPU_SET_INFORMATION,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::{
    GetProcessAffinityMask, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SET_INFORMATION, SetProcessAffinityMask,
};

const HEARTBEAT_STALE_PERIOD_SECONDS: i64 = 10;

#[derive(Debug, Clone, PartialEq)]
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
        info!("Refreshing all system info.");

        let mut worker_operations = WorkerOperations {
            sqlite: sqlite_pool,
        };
        loop {
            _ = run_worker_logic(&mut worker_operations, &mut system_info, &worker_status).await;
        }
    })
}

pub(crate) async fn run_worker_logic<WOps: WorkerOperations_>(
    worker_operations: &mut WOps,
    system_info: &mut System,
    worker_status: &watch::Sender<Option<WorkerHeartbeat>>,
) -> ResultBtAny<()> {
    worker_operations.sleep().await;

    let persistent_store = worker_operations
        .load_store(system_info)
        .await
        .inspect_err(|e| {
            let is_synced = None;
            let e = Some(e.get().to_string());
            worker_status.send_replace(Some(WorkerHeartbeat::now(is_synced, e)));
        })?;

    system_info.refresh_processes(ProcessesToUpdate::All, true);
    info!("Refreshing system process info.");

    let iracing_simulators =
        worker_operations.get_processes_by_exact_name(system_info, &persistent_store.simulator);
    let are_any_simulators = !iracing_simulators.is_empty();

    let simulator_spawners =
        worker_operations.get_processes_by_exact_name(system_info, &persistent_store.spawner);
    let are_any_spawners = !simulator_spawners.is_empty();

    match (are_any_simulators, are_any_spawners) {
        (false, false) => {
            let is_synced = None;
            let e = None;
            worker_status.send_replace(Some(WorkerHeartbeat::now(is_synced, e)));
        }
        (false, true) => {
            let are_spawners_synced = worker_operations
                .get_are_processes_synced(
                    &simulator_spawners,
                    (&persistent_store).into(),
                    &system_info,
                )
                .await
                .inspect_err(|e| {
                    let is_synced = None;
                    let e = Some(e.get().to_string());
                    worker_status.send_replace(Some(WorkerHeartbeat::now(is_synced, e)));
                })?;
            if are_spawners_synced {
                let is_synced = Some(true);
                let e = None;
                worker_status.send_replace(Some(WorkerHeartbeat::now(is_synced, e)));
            } else {
                let e = worker_operations
                    .set_processes_affinity(&simulator_spawners, (&persistent_store).into())
                    .await
                    .err()
                    .map(|e| e.get().to_string());
                let is_synced = Some(e.is_none());
                worker_status.send_replace(Some(WorkerHeartbeat::now(is_synced, e)));
            }
        }
        (true, _) => {
            let are_simulators_synced = worker_operations
                .get_are_processes_synced(
                    &iracing_simulators,
                    (&persistent_store).into(),
                    &system_info,
                )
                .await;
            let is_synced = are_simulators_synced.as_ref().ok().copied();
            let e = are_simulators_synced.err().map(|e| e.get().to_string());
            worker_status.send_replace(Some(WorkerHeartbeat::now(is_synced, e)));

            if are_any_spawners {
                _ = worker_operations
                    .set_processes_affinity(
                        &simulator_spawners,
                        &CpuSelections::new_all_selected(system_info.cpus().len()),
                    )
                    .await
                    // TODO: Expand heartbeat to send multiple errors, change frequency too.
                    .inspect_err(|e| error!("{:?}", e));
            }
        }
    }

    Ok(())
}

struct WorkerOperations {
    sqlite: SqlitePool,
}

pub(crate) trait WorkerOperations_ {
    async fn sleep(&mut self);
    async fn load_store(&mut self, system_info: &System) -> ResultBtAny<PersistentStore>;
    fn get_processes_by_exact_name(&mut self, system_info: &System, name: &str) -> Vec<IrAProcess>;
    async fn get_are_processes_synced(
        &mut self,
        candidate_processes: &[IrAProcess],
        cpu_selections: &CpuSelections,
        system_info: &System,
    ) -> ResultBtAny<bool>;
    async fn set_processes_affinity(
        &mut self,
        candidate_processes: &[IrAProcess],
        cpu_selections: &CpuSelections,
    ) -> ResultBtAny<()>;
}

impl WorkerOperations_ for WorkerOperations {
    async fn sleep(&mut self) {
        let worker_period = std::time::Duration::from_secs(WORKER_COOLDOWN_PERIOD_SECONDS);
        tokio::time::sleep(worker_period).await;
    }

    async fn load_store(&mut self, system_info: &System) -> ResultBtAny<PersistentStore> {
        PersistentStore::load(system_info.cpus().len(), &self.sqlite).await
    }

    fn get_processes_by_exact_name(
        &mut self,
        system_info: &System,
        exact_name: &str,
    ) -> Vec<IrAProcess> {
        system_info
            .processes_by_exact_name(OsStr::new(exact_name))
            .map(|process| process.into())
            .collect()
    }

    async fn get_are_processes_synced(
        &mut self,
        candidate_processes: &[IrAProcess],
        cpu_selections: &CpuSelections,
        system_info: &System,
    ) -> ResultBtAny<bool> {
        let cpu_selections = cpu_selections.into();
        get_are_processes_affinity_synced(candidate_processes, cpu_selections, system_info).await
    }

    async fn set_processes_affinity(
        &mut self,
        candidate_processes: &[IrAProcess],
        cpu_selections: &CpuSelections,
    ) -> ResultBtAny<()> {
        let cpu_selections = cpu_selections.into();
        for candidate_process in candidate_processes {
            set_cpu_affinity_of_process(candidate_process, cpu_selections).await?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct IrAProcess {
    #[allow(dead_code)]
    pub id: u32,
}

impl From<&Process> for IrAProcess {
    fn from(value: &Process) -> Self {
        Self {
            id: value.pid().as_u32(),
        }
    }
}

async fn get_are_processes_affinity_synced(
    candidate_processes: &[IrAProcess],
    cpu_selections: &CpuSelections,
    system_info: &System,
) -> ResultBtAny<bool> {
    for simulator_spawner in candidate_processes.iter() {
        let cpu_affinity = get_cpu_affinity_of_process(simulator_spawner)?;
        let cpu_selections_ = CpuSelections::new_preselected(
            mask_to_hashset(&cpu_affinity),
            system_info.cpus().len(),
        );
        let isnt_synced = *cpu_selections != cpu_selections_;
        if isnt_synced {
            return Ok(false);
        }
    }

    Ok(true)
}

fn get_cpu_affinity_of_process(
    #[allow(unused_variables)] process: &IrAProcess,
) -> ResultBtAny<usize> {
    #[cfg(target_os = "windows")]
    unsafe {
        let should_inherit_handle = false;
        let process = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION,
            should_inherit_handle,
            process.id,
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

    #[cfg(not(target_os = "windows"))]
    unimplemented!()
}

#[cfg(target_os = "windows")]
async fn get_cpu_sets() -> ResultBtAny<Vec<SYSTEM_CPU_SET_INFORMATION>> {
    let payload_size = std::mem::size_of::<SYSTEM_CPU_SET_INFORMATION>();

    let mut cpu_sets: [SYSTEM_CPU_SET_INFORMATION; 64] =
        [SYSTEM_CPU_SET_INFORMATION::default(); 64];
    let mut output_size: u32 = 0;
    let subject_process = None;
    let reserved_flag = 0;
    unsafe {
        let is_success = GetSystemCpuSetInformation(
            Some(cpu_sets.as_mut_ptr()),
            (cpu_sets.len() * payload_size) as u32,
            &mut output_size,
            subject_process,
            Some(reserved_flag),
        );
        info!("Got CPU set info.");
        if (!is_success).into() {
            Err(format!(
                "WinAPI call failed with code `{}`.",
                GetLastError().0
            ))?;
        }
    };
    let are_no_sets = output_size == 0;
    if are_no_sets {
        Err("There are no CPU sets!?")?;
    }

    let output_length = output_size as usize / payload_size;

    Ok(cpu_sets[0..output_length].to_vec())
}

async fn set_cpu_affinity_of_process(
    #[allow(unused_variables)] process: &IrAProcess,
    cpu_selections: &CpuSelections,
) -> ResultBtAny<()> {
    #[allow(unused_variables)]
    let cpu_selections = cpu_selections.to_mask();

    #[cfg(target_os = "windows")]
    {
        let process = unsafe {
            let should_inherit_handle = false;
            let process = OpenProcess(PROCESS_SET_INFORMATION, should_inherit_handle, process.id)?;
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

        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    unimplemented!()
}

#[tokio::test]
#[cfg(target_os = "windows")]
async fn getting_cpu_sets() {
    let cpu_sets: Vec<_> = get_cpu_sets().await.unwrap();
    let system_info = System::new_all();
    assert_eq!(cpu_sets.len(), system_info.cpus().len());
}
