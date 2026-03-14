use std::process::Command;

use sysinfo::System;
use tokio::sync::watch;

use crate::{
    errors::ResultBtAny,
    ir::{DEFAULT_IRACING_SIMULATOR, DEFAULT_SIMULATOR_SPAWNERS},
    persistence::{CpuSelections, PersistentStore},
    worker::{IrAProcess, WorkerHeartbeat, WorkerOperations_, run_worker_logic},
};

#[tokio::test]
async fn running_worker_logic_when_only_spawners_exist() {
    struct TestOperations {
        slept: usize,
        spawners: Option<CpuSelections>,
        simulations: Option<CpuSelections>,
    };
    impl WorkerOperations_ for TestOperations {
        async fn sleep(&mut self) {
            self.slept += 1;
        }

        async fn load_store(&mut self, system_info: &System) -> ResultBtAny<PersistentStore> {
            Ok(PersistentStore {
                simulator: String::from(DEFAULT_IRACING_SIMULATOR),
                selections: CpuSelections::new_evens_selected(12),
            })
        }

        fn get_processes_by_exact_name(
            &mut self,
            system_info: &System,
            name: &str,
        ) -> Vec<IrAProcess> {
            match name {
                DEFAULT_SIMULATOR_SPAWNERS => vec![IrAProcess { id: 7 }],
                DEFAULT_IRACING_SIMULATOR => vec![],
                _ => vec![],
            }
        }

        async fn get_are_processes_synced(
            &mut self,
            candidate_processes: &[IrAProcess],
            cpu_selections: &CpuSelections,
            system_info: &System,
        ) -> ResultBtAny<bool> {
            match candidate_processes[0].id {
                7 => Ok(self.spawners.as_ref() == Some(cpu_selections)),
                _ => panic!(),
            }
        }

        async fn set_processes_affinity(
            &mut self,
            candidate_processes: &[IrAProcess],
            cpu_selections: &CpuSelections,
        ) -> ResultBtAny<()> {
            match candidate_processes[0].id {
                7 => self.spawners = Some(cpu_selections.clone()),
                _ => panic!(),
            };
            Ok(())
        }
    }

    let mut worker_operations = TestOperations {
        slept: 0,
        spawners: None,
        simulations: None,
    };
    let mut system_info = System::new();
    let (status_tx, mut status_rx) = watch::channel(None);

    run_worker_logic(&mut worker_operations, &mut system_info, &status_tx).await;
    assert_eq!(worker_operations.slept, 1);
    assert_eq!(
        worker_operations.spawners,
        Some(CpuSelections::new_evens_selected(12))
    );
    assert_eq!(worker_operations.simulations, None);
    let first_beat = status_rx
        .wait_for(|status| status.is_some())
        .await
        .unwrap()
        .clone();
    assert!(first_beat.is_some());

    run_worker_logic(&mut worker_operations, &mut system_info, &status_tx).await;
    assert_eq!(worker_operations.slept, 2);
    assert_eq!(
        worker_operations.spawners,
        Some(CpuSelections::new_evens_selected(12))
    );
    assert_eq!(worker_operations.simulations, None);
    let second_beat = status_rx.wait_for(|status| status.is_some()).await.unwrap();
    assert!(second_beat.is_some());

    assert!(first_beat != *second_beat);
}

#[tokio::test]
async fn running_worker_logic_when_both_processes_exist() {
    struct TestOperations {
        slept: usize,
        spawners: Option<CpuSelections>,
        simulations: Option<CpuSelections>,
    };
    impl WorkerOperations_ for TestOperations {
        async fn sleep(&mut self) {
            self.slept += 1;
        }

        async fn load_store(&mut self, system_info: &System) -> ResultBtAny<PersistentStore> {
            Ok(PersistentStore {
                simulator: String::from(DEFAULT_IRACING_SIMULATOR),
                selections: CpuSelections::new_evens_selected(12),
            })
        }

        fn get_processes_by_exact_name(
            &mut self,
            system_info: &System,
            name: &str,
        ) -> Vec<IrAProcess> {
            match name {
                DEFAULT_SIMULATOR_SPAWNERS => vec![IrAProcess { id: 7 }],
                DEFAULT_IRACING_SIMULATOR => vec![IrAProcess { id: 13 }],
                _ => vec![],
            }
        }

        async fn get_are_processes_synced(
            &mut self,
            candidate_processes: &[IrAProcess],
            cpu_selections: &CpuSelections,
            system_info: &System,
        ) -> ResultBtAny<bool> {
            match candidate_processes[0].id {
                7 => Ok(self.spawners.as_ref() == Some(cpu_selections)),
                13 => Ok(self.simulations.as_ref() == Some(cpu_selections)),
                _ => panic!(),
            }
        }

        async fn set_processes_affinity(
            &mut self,
            candidate_processes: &[IrAProcess],
            cpu_selections: &CpuSelections,
        ) -> ResultBtAny<()> {
            match candidate_processes[0].id {
                7 => self.spawners = Some(cpu_selections.clone()),
                13 => self.simulations = Some(cpu_selections.clone()),
                _ => panic!(),
            };
            Ok(())
        }
    }

    let mut worker_operations = TestOperations {
        slept: 0,
        spawners: None,
        simulations: None,
    };
    let mut system_info = System::new();
    let (status_tx, mut status_rx) = watch::channel(None);

    run_worker_logic(&mut worker_operations, &mut system_info, &status_tx).await;
    assert_eq!(worker_operations.slept, 1);
    assert_eq!(
        worker_operations.spawners,
        Some(CpuSelections::new_all_selected(12))
    );
    assert_eq!(worker_operations.simulations, None);
    let first_beat = status_rx
        .wait_for(|status| status.is_some())
        .await
        .unwrap()
        .clone();
    assert!(first_beat.is_some());

    run_worker_logic(&mut worker_operations, &mut system_info, &status_tx).await;
    assert_eq!(worker_operations.slept, 2);
    assert_eq!(
        worker_operations.spawners,
        Some(CpuSelections::new_all_selected(12))
    );
    assert_eq!(worker_operations.simulations, None);
    let second_beat = status_rx.wait_for(|status| status.is_some()).await.unwrap();
    assert!(second_beat.is_some());

    assert!(first_beat != *second_beat);
}
