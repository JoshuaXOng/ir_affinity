use sqlx::SqlitePool;
use sysinfo::{Process, System};
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

pub const IRA_WORKER_NAME: &str = "ir_affinity.exe";

#[derive(Debug, Clone)]
pub enum RunningStatus {
    None,
    One,
    Many,
}

pub fn get_worker_status(system_info: &System) -> RunningStatus {
    let iracing_simulators: Vec<&Process> = system_info
        .processes_by_exact_name(IRA_WORKER_NAME.as_ref())
        .collect();
    info!("Got all Ir Affinity processes.");

    match iracing_simulators.len() {
        0 => RunningStatus::None,
        1 => RunningStatus::One,
        _ => RunningStatus::Many,
    }
}

pub async fn get_are_simulators_affinity_synced(system_info: &System, sqlite_pool: &SqlitePool) -> ResultBtAny<bool> {
    let persistent_store = PersistentStore::load(system_info, sqlite_pool).await?;

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

pub async fn sync_simulators_affinity(
    system_info: &System,
    sqlite_pool: &SqlitePool,
) -> ResultBtAny<()> {
    let persistent_store = PersistentStore::load(system_info, sqlite_pool).await?;

    let iracing_simulators: Vec<&Process> = system_info
        .processes_by_exact_name(persistent_store.process.as_ref())
        .collect();

    for iracing_simulator in iracing_simulators {
        set_cpu_affinity_of_process(iracing_simulator, system_info, sqlite_pool).await?;
    }

    Ok(())
}

pub async fn set_cpu_affinity_of_process(
    process: &Process,
    system_info: &System,
    sqlite_pool: &SqlitePool,
) -> ResultBtAny<()> {
    let persistent_store = PersistentStore::load(system_info, sqlite_pool).await?;
    let cpu_selections = persistent_store.selections.to_mask();

    #[cfg(target_os = "windows")]
    {
        // TODO: Do I need to close handle else memory leak?
        // Implement Drop wrapper?
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
