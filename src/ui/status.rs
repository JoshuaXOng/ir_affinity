use crate::worker::WorkerHeartbeat;
use iced::font::Weight;
use iced::widget::{column, row, text};
use iced::{Element, Font, Length, Subscription};
use tokio::sync::watch;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::WatchStream;

#[derive(Debug, Clone)]
pub struct WorkerStatus {
    last: Option<WorkerHeartbeat>,
    progress: usize,
    error: Option<String>,
}

impl WorkerStatus {
    pub fn new() -> Self {
        Self {
            last: None,
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
        let progress_ellipses = self.get_progress_ellipses();

        let running_status = if let Some(last_heartbeat) = &self.last {
            if !last_heartbeat.get_is_stale() {
                text(format!("Running{}", progress_ellipses)).style(text::success)
            } else {
                text(format!("Lost connection{}", progress_ellipses)).style(text::warning)
            }
        } else {
            text(format!("Starting{}", progress_ellipses))
        };

        let configuration_status = if let Some(last_heartbeat) = &self.last {
            match (
                last_heartbeat.get_is_synced(),
                last_heartbeat.get_is_stale(),
            ) {
                (Some(true), true) => text("Likely syncd"),
                (Some(true), false) => text("Synced").style(text::success),
                (Some(false), true) => text("Likely unsynced").style(text::warning),
                (Some(false), false) => text("Unsynced").style(text::warning),
                (None, true) | (None, false) => text("N/A"),
            }
        } else {
            text("N/A")
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

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Heatbeat(worker_heartbeat) => {
                self.last = worker_heartbeat.clone();
                self.error = self.last.as_ref().and_then(|beat| beat.get_error().clone());
            }
            Message::Progress => {
                self.progress = self.progress.wrapping_add(1);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Heatbeat(Option<WorkerHeartbeat>),
    Progress,
}

#[allow(clippy::type_complexity)]
fn watch_worker_status(
    worker_status: &ReceiverWrapper,
) -> tokio_stream::adapters::Map<
    WatchStream<Option<WorkerHeartbeat>>,
    fn(Option<WorkerHeartbeat>) -> Message,
> {
    WatchStream::from_changes(worker_status.1.clone()).map(Message::Heatbeat)
}

struct ReceiverWrapper(usize, watch::Receiver<Option<WorkerHeartbeat>>);

impl std::hash::Hash for ReceiverWrapper {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

const PROGRESS_COOLDOWN_SECONDS: u64 = 1;

const STATUS_HEARTBEAT_RECEIVER_SLOT: usize = 0;

pub fn get_subscriptions(
    worker_status: &watch::Receiver<Option<WorkerHeartbeat>>,
) -> Subscription<Message> {
    Subscription::batch([
        iced::time::every(std::time::Duration::from_secs(PROGRESS_COOLDOWN_SECONDS))
            .map(|_| Message::Progress),
        Subscription::run_with(
            ReceiverWrapper(STATUS_HEARTBEAT_RECEIVER_SLOT, worker_status.clone()),
            watch_worker_status,
        ),
    ])
}
