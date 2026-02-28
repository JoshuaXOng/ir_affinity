use std::process::Command;

use sysinfo::System;
use tokio::sync::watch;

use crate::{
    errors::ResultBtAny,
    ir::DEFAULT_IRACING_SIMULATOR,
    persistence::{CpuSelections, PersistentStore},
    worker::{IrAProcess, WorkerHeartbeat, WorkerOperations_, run_worker_logic},
};

#[tokio::test]
async fn running_worker_logic() {
    struct TestOperations {
        slept: usize,
        synced: (bool, usize),
    };
    impl WorkerOperations_ for TestOperations {
        async fn sleep(&mut self) {
            self.slept += 1;
        }

        async fn load_store(&mut self, system_info: &System) -> ResultBtAny<PersistentStore> {
            Ok(PersistentStore {
                process: String::from(DEFAULT_IRACING_SIMULATOR),
                selections: CpuSelections::new_all_selected(2),
            })
        }

        fn get_processes_by_exact_name(
            &mut self,
            system_info: &System,
            name: &str,
        ) -> Vec<IrAProcess> {
            vec![IrAProcess { id: 42 }]
        }

        async fn get_are_synced(
            &mut self,
            persistent_store: &PersistentStore,
            system_info: &System,
        ) -> ResultBtAny<bool> {
            Ok(self.synced.0)
        }

        async fn sync_simulators(
            &mut self,
            persistent_store: &PersistentStore,
            system_info: &System,
        ) -> ResultBtAny<()> {
            self.synced.0 = true;
            self.synced.1 += 1;
            Ok(())
        }
    }

    let mut worker_operations = TestOperations {
        slept: 0,
        synced: (false, 0),
    };
    let mut system_info = System::new();
    let (status_tx, mut status_rx) = watch::channel(None);

    run_worker_logic(&mut worker_operations, &mut system_info, &status_tx).await;
    assert_eq!(worker_operations.slept, 1);
    assert_eq!(worker_operations.synced, (true, 1));
    let first_beat = status_rx
        .wait_for(|status| status.is_some())
        .await
        .unwrap()
        .clone();
    assert!(first_beat.is_some());

    run_worker_logic(&mut worker_operations, &mut system_info, &status_tx).await;
    assert_eq!(worker_operations.slept, 2);
    assert_eq!(worker_operations.synced, (true, 1));
    let second_beat = status_rx.wait_for(|status| status.is_some()).await.unwrap();
    assert!(second_beat.is_some());

    assert!(first_beat != *second_beat);
}
