use std::{collections::HashSet, fmt::Display, fs, path::PathBuf, str::FromStr};

use directories::ProjectDirs;
use iced::futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};
use tracing::info;

use crate::{errors::ResultBtAny, ir::DEFAULT_IRACING_SIMULATOR, selections::hashset_to_mask};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentStore {
    pub process: String,
    pub selections: CpuSelections,
}

impl PersistentStore {
    const CONFIGURATION_FILENAME: &str = "store.sqlite";

    const MAIN_PROCESS_ID: u32 = 0;

    pub fn get_configuration_file() -> ResultBtAny<PathBuf> {
        Ok(get_configuration_directory()?.join(Self::CONFIGURATION_FILENAME))
    }

    pub async fn load(cpu_count: usize, sqlite_pool: &SqlitePool) -> ResultBtAny<PersistentStore> {
        let process = sqlx::query!(
            "SELECT name FROM processes WHERE id = ?1",
            Self::MAIN_PROCESS_ID
        )
        .fetch_optional(sqlite_pool)
        .await?;
        info!("Queried simulation process name.");

        let self_ = if let Some(process) = process {
            let mut cpu_selections = HashSet::new();

            let mut relations = sqlx::query!(
                "SELECT cpu_id FROM processes_selected_cpus WHERE process_id = ?1",
                Self::MAIN_PROCESS_ID
            )
            .fetch(sqlite_pool);
            info!("Queried selected CPUs.");
            while let Some(relation) = relations.try_next().await? {
                cpu_selections.insert(relation.cpu_id.try_into()?);
            }

            Self {
                process: process.name,
                selections: CpuSelections::new_preselected(cpu_selections, cpu_count),
            }
        } else {
            Self {
                process: DEFAULT_IRACING_SIMULATOR.to_string(),
                selections: CpuSelections::new_all_selected(cpu_count),
            }
        };

        Ok(self_)
    }

    pub async fn create_pool() -> ResultBtAny<SqlitePool> {
        let to_sqlite = Self::get_configuration_file()?;
        if let Some(parent) = to_sqlite.parent() {
            fs::create_dir_all(parent)?;
            info!("Created directories to `{}`.", parent.to_string_lossy());
        }
        let connection_options = SqliteConnectOptions::from_str(
            format!("sqlite://{}", to_sqlite.to_string_lossy()).as_str(),
        )?
        .journal_mode(SqliteJournalMode::Wal);
        let sqlite_pool = SqlitePool::connect_with(connection_options).await?;
        info!("Connected to SQLite.");

        Ok(sqlite_pool)
    }

    pub async fn create_ddl(sqlite_pool: &SqlitePool) -> ResultBtAny<()> {
        sqlx::migrate!("./migrations").run(sqlite_pool).await?;
        info!("Ran migrations.");
        Ok(())
    }

    pub async fn save(&self, sqlite_pool: &SqlitePool) -> ResultBtAny<()> {
        let mut transaction = sqlite_pool.begin().await?;

        sqlx::query!(
            r#"
            DELETE FROM processes_selected_cpus
            WHERE process_id = ?1;
            "#,
            Self::MAIN_PROCESS_ID,
        )
        .execute(&mut *transaction)
        .await?;
        info!("Deleted selected CPUs.");

        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO processes (id, name)
            VALUES (?1, ?2);
            "#,
            Self::MAIN_PROCESS_ID,
            self.process
        )
        .execute(&mut *transaction)
        .await?;
        info!("Inserted process name.");
        for &cpu_selection in self.selections.inner.iter() {
            let cpu_selection = u32::try_from(cpu_selection)?;

            sqlx::query!(
                r#"
                INSERT OR REPLACE INTO cpus (id)
                VALUES (?1);
                "#,
                cpu_selection
            )
            .execute(&mut *transaction)
            .await?;
            info!("Inserted existing CPUs.");

            sqlx::query!(
                r#"
                INSERT OR REPLACE INTO processes_selected_cpus (process_id, cpu_id)
                VALUES (?1, ?2);
                "#,
                Self::MAIN_PROCESS_ID,
                cpu_selection,
            )
            .execute(&mut *transaction)
            .await?;
            info!("Created selected CPUs relationship.");
        }

        transaction.commit().await?;

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq)]
pub struct CpuSelections {
    inner: HashSet<usize>,
    cpu_count: usize,
}

impl CpuSelections {
    pub const DISPLAY_TITLE: &str = "CPUs: ";
    const NONE_DISPLAY: &str = "None";

    pub fn new(cpu_count: usize) -> Self {
        Self {
            inner: Default::default(),
            cpu_count,
        }
    }

    pub fn new_preselected(cpu_selections: HashSet<usize>, cpu_count: usize) -> Self {
        Self {
            inner: cpu_selections,
            cpu_count,
        }
    }

    pub fn new_all_selected(cpu_count: usize) -> Self {
        let mut cpu_selections = HashSet::new();
        for cpu_selection in 0..cpu_count {
            cpu_selections.insert(cpu_selection);
        }
        Self {
            inner: cpu_selections,
            cpu_count,
        }
    }

    pub fn get_is_selected(&self, cpu_id: &usize) -> bool {
        self.inner.contains(cpu_id)
    }

    pub fn get_nonselected_string() -> String {
        String::from(Self::DISPLAY_TITLE) + Self::NONE_DISPLAY
    }

    pub fn get_cpu_count(&self) -> usize {
        self.cpu_count
    }

    pub fn toggle_selection(&mut self, cpu_id: usize, should_activate: bool) -> ResultBtAny<()> {
        let is_over = cpu_id >= self.cpu_count;
        if is_over {
            Err(format!(
                "CPU selection of `{}` is out of bounds `[0, {})`.",
                cpu_id, self.cpu_count
            ))?
        }

        if !should_activate {
            self.inner.remove(&cpu_id);
        } else {
            self.inner.insert(cpu_id);
        };
        Ok(())
    }

    pub fn to_mask(&self) -> usize {
        hashset_to_mask(&self.inner)
    }
}

impl PartialEq for CpuSelections {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Display for CpuSelections {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Self::DISPLAY_TITLE)?;

        let is_none = self.inner.is_empty();
        let is_all = self.inner.len() == self.cpu_count;
        if is_none {
            write!(f, "{}", Self::NONE_DISPLAY)
        } else if is_all {
            write!(f, "All ({})", self.cpu_count)
        } else {
            let mut cpu_selections: Vec<_> = self.inner.iter().collect();
            cpu_selections.sort();
            write!(
                f,
                "{}",
                cpu_selections
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )?;
            Ok(())
        }
    }
}

pub fn get_configuration_directory() -> ResultBtAny<PathBuf> {
    let project_directories =
        ProjectDirs::from("com", "jxo", "ir_affinity").ok_or("Could not get `ProjectDirs`.")?;

    Ok(project_directories.config_local_dir().to_path_buf())
}
