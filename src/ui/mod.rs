use crate::ir::{DEFAULT_IRACING_SIMULATOR, DEFAULT_SIMULATOR_SPAWNERS};
use crate::persistence::PersistentStore;
pub use crate::ui::errors::run_error_ui;
use crate::worker::WorkerHeartbeat;
use iced::font::Weight;
use iced::widget::{button, column, rule, scrollable, text, text_input};
use iced::{Alignment, Element, Font, Length, Subscription, Task};
use sqlx::SqlitePool;
use status::WorkerStatus;
use tokio::sync::watch;
use tracing::error;

mod errors;
mod selection;
mod status;

const MAIN_WINDOW_NAME: &str = "Ir Affinity";

const INITIAL_WINDOW_SIZE: (u32, u32) = (400, 420);

const IS_WINDOW_RESIZABLE: bool = false;

pub fn run_initialized_ui(
    persistent_store: PersistentStore,
    sqlite_pool: SqlitePool,
    status_receiver: watch::Receiver<Option<WorkerHeartbeat>>,
) -> iced::Result {
    iced::application(
        move || IrAffinity::new(&persistent_store, &sqlite_pool),
        IrAffinity::update,
        IrAffinity::view,
    )
    .title(MAIN_WINDOW_NAME)
    .window_size(INITIAL_WINDOW_SIZE)
    .resizable(IS_WINDOW_RESIZABLE)
    .subscription(move |_| {
        let subscriptions = vec![
            selection::get_subscriptions().map(Message::CpuSelection),
            status::get_subscriptions(&status_receiver).map(Message::WorkerStatus),
            get_subscriptions(),
        ];
        Subscription::batch(subscriptions)
    })
    .run()
}

struct IrAffinity {
    spawner_name: String,
    simulator_name: String,
    cpu_selection: selection::CpuSelection,
    worker_status: status::WorkerStatus,
    sqlite: SqlitePool,
    is_saving: bool,
    progress: usize,
    error: Option<String>,
}

impl IrAffinity {
    fn new(persistent_store: &PersistentStore, sqlite_pool: &SqlitePool) -> Self {
        Self {
            spawner_name: persistent_store.spawner.clone(),
            simulator_name: persistent_store.simulator.clone(),
            cpu_selection: selection::CpuSelection::new(persistent_store.selections.clone()),
            worker_status: WorkerStatus::new(),
            sqlite: sqlite_pool.clone(),
            progress: 0,
            is_saving: false,
            error: None,
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let error_message = self.error.clone().map(|e| text(e).style(text::danger));

        let bold = Font {
            weight: Weight::Bold,
            ..Font::default()
        };

        let spawner_component = {
            column![
                text("iRacing UI").size(16).font(bold),
                text_input(DEFAULT_SIMULATOR_SPAWNERS, &self.spawner_name)
                    .on_input(Message::ChangedSpawner)
                    .size(16)
            ]
            .spacing(4)
        };

        let simulation_component = {
            column![
                text("iRacing Simulation").size(16).font(bold),
                text_input(DEFAULT_IRACING_SIMULATOR, &self.simulator_name)
                    .on_input(Message::ChangedSimulation)
                    .size(16)
            ]
            .spacing(4)
        };

        let selection_component = self.cpu_selection.view().map(Message::CpuSelection);

        let ellipses = ".".repeat((self.progress / 5) % 3 + 1);
        let save_button = if self.is_saving {
            button(text(format!("Saving{ellipses}")))
        } else {
            button("Save").on_press(Message::ShouldSave)
        };

        let status_component = self.worker_status.view().map(Message::WorkerStatus);

        scrollable(
            column![
                error_message,
                spawner_component,
                simulation_component,
                selection_component,
                save_button,
                rule::horizontal(2),
                status_component
            ]
            .width(Length::Fill)
            .spacing(16)
            .padding(16)
            .align_x(Alignment::Center),
        )
        .into()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ChangedSpawner(spawner_name) => {
                self.spawner_name = spawner_name;
                Task::none()
            }
            Message::ChangedSimulation(simulator_name) => {
                self.simulator_name = simulator_name;
                Task::none()
            }
            Message::CpuSelection(message) => {
                self.cpu_selection.update(message);
                Task::none()
            }
            Message::ShouldSave => {
                self.is_saving = true;

                let sqlite_pool = self.sqlite.clone();
                let spawner_name = self.spawner_name.clone();
                let simulator_name = self.simulator_name.clone();
                let cpu_selections = self.cpu_selection.get_inner().clone();
                Task::future(async move {
                    let is_success = PersistentStore {
                        spawner: spawner_name,
                        simulator: simulator_name,
                        selections: cpu_selections,
                    }
                    .save(&sqlite_pool)
                    .await
                    .inspect_err(|e| error!("{:?}", e))
                    // TODO: Update `bt_error` crate to have `Debug` and `Clone`.
                    .map_err(|e| e.get().to_string());
                    Message::ShouldSave_(is_success)
                })
            }
            Message::ShouldSave_(is_success) => {
                self.is_saving = false;
                self.error = is_success.err();
                Task::none()
            }
            Message::WorkerStatus(message) => {
                self.worker_status.update(message);
                Task::none()
            }
            Message::Progress => {
                self.progress = self.progress.wrapping_add(1);
                Task::none()
            }
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    ChangedSpawner(String),
    ChangedSimulation(String),
    CpuSelection(selection::Message),
    ShouldSave,
    ShouldSave_(Result<(), String>),
    WorkerStatus(status::Message),
    Progress,
}

const PROGRESS_COOLDOWN_MILLISECONDS: u64 = 100;

fn get_subscriptions() -> Subscription<Message> {
    let progress_period = std::time::Duration::from_millis(PROGRESS_COOLDOWN_MILLISECONDS);
    let subscriptions = vec![iced::time::every(progress_period).map(|_| Message::Progress)];
    Subscription::batch(subscriptions)
}
