use std::time::Duration;

use iced::font::Weight;
use iced::widget::{column, row, text};
use iced::{Element, Font, Length, Subscription, Task};
use ir_affinity::worker::{
    RunningStatus, get_are_simulators_affinity_synced, get_worker_status, sync_simulators_affinity,
};
use sqlx::SqlitePool;
use sysinfo::{CpuRefreshKind, ProcessesToUpdate, System};

#[derive(Debug, Clone)]
pub struct WorkerStatus {
    inner: WorkerStatus_,
    progress: usize,
    error: Option<String>,
}

impl WorkerStatus {
    pub fn new() -> Self {
        Self {
            inner: WorkerStatus_ {
                running_status: None,
                is_configured: None,
            },
            progress: 0,
            error: None,
        }
    }
}

impl WorkerStatus {
    fn get_progress_ellipses(&self) -> String {
        ".".repeat(self.progress % 3 + 1)
    }

    pub fn view(&self) -> Element<'_, Message> {
        let running_status =
            self.inner
                .running_status
                .as_ref()
                .map(|status| match status {
                    RunningStatus::None => text("None").style(text::warning),
                    RunningStatus::One => text(format!("Running{}", self.get_progress_ellipses()))
                        .style(text::success),
                    RunningStatus::Many => text("Too Many").style(text::warning),
                })
                .unwrap_or_else(|| text(format!("Setting Up{}", self.get_progress_ellipses())));

        let configuration_status = if let Some(RunningStatus::One) = self.inner.running_status {
            match self.inner.is_configured {
                Some(true) => text("Synced").style(text::success),
                Some(false) => {
                    text(format!("Syncing{}", self.get_progress_ellipses())).style(text::warning)
                }
                None => "Unknown".into(),
            }
        } else {
            "N/A".into()
        };

        let bold = Font {
            weight: Weight::Bold,
            ..Font::default()
        };
        let view = column![
            row![text("Worker Status: ").font(bold), running_status],
            row![text("Config Status: ").font(bold), configuration_status]
        ]
        .width(Length::Fill);

        view.into()
    }

    pub fn update(&mut self, message: Message, sqlite_pool: SqlitePool) -> Task<Message> {
        match message {
            Message::WorkerStatus => Task::future(async move {
                let mut system_info = System::new();
                system_info.refresh_cpu_list(CpuRefreshKind::nothing());
                system_info.refresh_processes(ProcessesToUpdate::All, true);

                match get_are_simulators_affinity_synced(&system_info, &sqlite_pool).await {
                    Ok(is_synced) => Message::WorkerStatus_(WorkerStatus_ {
                        running_status: Some(get_worker_status(&system_info)),
                        is_configured: Some(is_synced),
                    }),
                    Err(e) => Message::Error(e.get().to_string()),
                }
            }),
            Message::WorkerStatus_(worker_status) => {
                self.inner = worker_status;
                Task::none()
            }
            Message::Progress => {
                self.progress = self.progress.wrapping_add(1);
                Task::future(async move {
                    let mut system_info = System::new();
                    system_info.refresh_processes(ProcessesToUpdate::All, true);

                    if let Err(e) = sync_simulators_affinity(&system_info, &sqlite_pool).await {
                        return Message::Error(e.get().to_string());
                    }
                    Message::NoOperation
                })
            }
            Message::Error(error) => {
                self.error = Some(error);
                Task::none()
            }
            Message::NoOperation => Task::none(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Progress,
    WorkerStatus,
    WorkerStatus_(WorkerStatus_),
    Error(String),
    NoOperation,
}

#[derive(Debug, Clone)]
pub struct WorkerStatus_ {
    running_status: Option<RunningStatus>,
    is_configured: Option<bool>,
}

const WORKER_REFRESH_PERIOD_SECCONDS: u64 = 5;

pub fn get_subscriptions() -> Subscription<Message> {
    Subscription::batch([
        iced::time::every(Duration::from_secs(1)).map(|_| Message::Progress),
        iced::time::every(Duration::from_secs(WORKER_REFRESH_PERIOD_SECCONDS))
            .map(|_| Message::WorkerStatus),
    ])
}
