use std::time::Duration;

use iced::font::Weight;
use iced::futures::executor::block_on;
use iced::widget::{button, column, rule, scrollable, text, text_input};
use iced::{Center, Element, Font, Length, Subscription, Task};
use ir_affinity::ir::DEFAULT_IRACING_SIMULATOR;
use ir_affinity::persistence::{CpuSelections, PersistentStore};
use ir_affinity::unwrap_or;
use sqlx::SqlitePool;
use sysinfo::System;
use tracing::error;

use crate::selection::CpuSelection;
use crate::status::WorkerStatus;

mod selection;
mod status;

const MAIN_WINDOW_NAME: &str = "Ir Affinity";
const INITIAL_WINDOW_SIZE: (u32, u32) = (400, 350);

fn main() -> iced::Result {
    tracing_subscriber::fmt::init();

    iced::application(IrAffinity::default, IrAffinity::update, IrAffinity::view)
        .title(MAIN_WINDOW_NAME)
        .window_size(INITIAL_WINDOW_SIZE)
        .resizable(false)
        .subscription(|application| {
            let subscriptions = vec![
                selection::get_subscriptions().map(Message::CpuSelection),
                status::get_subscriptions().map(Message::WorkerStatus),
                get_subscriptions(application),
            ];
            Subscription::batch(subscriptions)
        })
        .run()
}

struct IrAffinity {
    simulator_name: Option<String>,
    cpu_selection: selection::CpuSelection,
    worker_status: status::WorkerStatus,
    sqlite_pool: Option<SqlitePool>,
    progress: usize,
    is_initializing: bool,
    is_saving: bool,
    error: Option<String>,
}

impl Default for IrAffinity {
    fn default() -> Self {
        let mut system_info = System::new();
        system_info.refresh_all();
        let cpu_count = system_info.cpus().len();
        Self {
            simulator_name: None,
            cpu_selection: CpuSelection::new(cpu_count),
            worker_status: WorkerStatus::new(),
            sqlite_pool: None,
            progress: 0,
            is_initializing: false,
            is_saving: false,
            error: None,
        }
    }
}

impl IrAffinity {
    fn get_should_initialize(&self) -> bool {
        !self.is_initializing && !self.get_is_initialized()
    }

    fn get_is_initialized(&self) -> bool {
        self.get_initialization().is_some()
    }

    fn get_initialization(&self) -> Option<(&str, &CpuSelections, &SqlitePool)> {
        match (
            &self.simulator_name,
            self.cpu_selection.get_initialization(),
            &self.sqlite_pool,
        ) {
            (Some(name), Some(selections), Some(pool)) => Some((name.as_str(), selections, pool)),
            _ => None,
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let error_message = self.error.clone().map(|e| text(e).style(text::danger));

        let process_component = {
            let bold = Font {
                weight: Weight::Bold,
                ..Font::default()
            };
            column![
                text("iRacing Simulation").size(16).font(bold),
                text_input(
                    DEFAULT_IRACING_SIMULATOR,
                    self.simulator_name
                        .as_ref()
                        .map(|name| name.as_str())
                        .unwrap_or("")
                )
                .on_input_maybe(if self.get_is_initialized() {
                    Some(|input| Message::ChangedText(input))
                } else {
                    None
                })
                .size(16)
            ]
            .spacing(4)
        };

        let selection_component = self.cpu_selection.view().map(Message::CpuSelection);

        let ellipses = ".".repeat((self.progress / 5) % 3 + 1);
        let save_button = if let Some((process_name, cpu_selections, _)) = self.get_initialization()
        {
            if self.is_saving {
                button(text(format!("Saving{ellipses}")))
            } else {
                button("Save").on_press(Message::ShouldSave {
                    process: process_name.to_string(),
                    selections: cpu_selections.clone(),
                })
            }
        } else {
            button(text(format!("Loading{ellipses}")))
        };

        let status_component = self.worker_status.view().map(Message::WorkerStatus);

        scrollable(
            column![
                error_message,
                process_component,
                selection_component,
                save_button,
                rule::horizontal(2),
                status_component
            ]
            .width(Length::Fill)
            .spacing(16)
            .padding(16)
            .align_x(Center),
        )
        .into()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Initialize => {
                if !self.get_should_initialize() {
                    return Task::none();
                }
                self.is_initializing = true;
                Task::future(async {
                    let sqlite_pool = unwrap_or!(PersistentStore::create_pool().await, e, {
                        return Message::InitializeFailed(e.get().to_string());
                    });

                    if let Err(e) = block_on(PersistentStore::create_ddl(&sqlite_pool)) {
                        return Message::InitializeFailed(e.get().to_string());
                    }

                    let mut system_info = System::new();
                    system_info.refresh_all();
                    match PersistentStore::load(&system_info, &sqlite_pool).await {
                        Ok(persistence) => Message::Initialize_(persistence, sqlite_pool),
                        Err(e) => Message::InitializeFailed(format!(
                            "Failed to load persistence store. {}",
                            e.get()
                        )),
                    }
                })
            }
            Message::Initialize_(persistence, sqlite_pool) => {
                self.is_initializing = false;
                self.sqlite_pool = Some(sqlite_pool.clone());
                self.simulator_name = Some(persistence.process);
                Task::done(Message::CpuSelection(selection::Message::Initialize(
                    persistence.selections,
                )))
            }
            Message::InitializeFailed(e) => {
                self.is_initializing = false;
                self.error = Some(e);
                Task::none()
            }
            Message::Progress => {
                self.progress = self.progress.wrapping_add(1);
                Task::none()
            }
            Message::ChangedText(simulator_name) => {
                self.simulator_name = Some(simulator_name);
                Task::none()
            }
            Message::CpuSelection(message) => {
                self.cpu_selection.update(message);
                Task::none()
            }
            Message::ShouldSave {
                process,
                selections,
            } => {
                self.is_saving = true;
                let sqlite_pool = unwrap_or!(&self.sqlite_pool, {
                    return Task::done(Message::ShouldSave_(Err(String::from(
                        "SQLite not initialized yet.",
                    ))));
                })
                .clone();

                Task::future(async move {
                    let is_success = PersistentStore {
                        process,
                        selections,
                    }
                    .save(&sqlite_pool)
                    .await
                    .inspect_err(|e| error!("{:?}", e))
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
                if let Some((_, _, sqlite_pool)) = self.get_initialization() {
                    self.worker_status
                        .update(message, sqlite_pool.clone())
                        .map(Message::WorkerStatus)
                } else {
                    self.error = Some(String::from("SQLite not initialized."));
                    Task::none()
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    Initialize,
    // TODO: Update `bt_error` crate to have `Debug` and `Clone`.
    Initialize_(PersistentStore, SqlitePool),
    InitializeFailed(String),
    Progress,
    ChangedText(String),
    CpuSelection(selection::Message),
    ShouldSave {
        process: String,
        selections: CpuSelections,
    },
    ShouldSave_(Result<(), String>),
    WorkerStatus(status::Message),
}

fn get_subscriptions(self_: &IrAffinity) -> Subscription<Message> {
    let progress_period = Duration::from_millis(100);
    let mut subscriptions = vec![iced::time::every(progress_period).map(|_| Message::Progress)];

    if self_.get_should_initialize() {
        let cooldown_period = Duration::from_millis(100);
        subscriptions.push(iced::time::every(cooldown_period).map(|_| Message::Initialize))
    }
    Subscription::batch(subscriptions)
}
