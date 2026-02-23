use iced::font::Weight;
use iced::widget::{column, row, text};
use iced::{Element, Font, Length, Subscription};
use crate::worker::WorkerHeartbeat;
use tokio::sync::watch;

#[derive(Debug, Clone)]
pub struct WorkerStatus {
    last: Option<WorkerHeartbeat>,
    inner: watch::Receiver<Option<WorkerHeartbeat>>,
    progress: usize,
    error: Option<String>,
}

impl WorkerStatus {
    pub fn new(worker_status: watch::Receiver<Option<WorkerHeartbeat>>) -> Self {
        Self {
            last: None,
            inner: worker_status,
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
                (true, true) => text("Likely syncd"),
                (true, false) => text("Synced").style(text::success),
                (false, true) => text("Likely unsynced").style(text::warning),
                (false, false) => text("Unsynced").style(text::warning),
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
            Message::Progress => {
                self.progress = self.progress.wrapping_add(1);
                self.last = self.inner.borrow().clone();
                self.error = self.last.as_ref().and_then(|beat| beat.get_error().clone());
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Progress,
}

const PROGRESS_COOLDOWN_SECONDS: u64 = 1;

pub fn get_subscriptions() -> Subscription<Message> {
    Subscription::batch([iced::time::every(std::time::Duration::from_secs(
        PROGRESS_COOLDOWN_SECONDS,
    ))
    .map(|_| Message::Progress)])
}
