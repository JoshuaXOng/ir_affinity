use std::time::Duration;

use iced::Alignment;
use iced::Background;
use iced::Color;
use iced::Element;
use iced::Font;
use iced::Length;
use iced::Subscription;
use iced::alignment::Horizontal;
use iced::alignment::Vertical;
use iced::font::Weight;
use iced::widget::scrollable::Scrollbar;
use iced::widget::{checkbox, column, container, scrollable, text};
use ir_affinity::persistence::CpuSelections;
use ir_affinity::unwrap_or;

pub struct CpuSelection {
    selections: Option<CpuSelections>,
    cpu_count: usize,
    progress: usize,
    error: Option<String>,
}

impl CpuSelection {
    pub fn new(cpu_count: usize) -> Self {
        Self {
            selections: None,
            cpu_count,
            progress: 0,
            error: None,
        }
    }
}

impl CpuSelection {
    pub fn get_initialization(&self) -> Option<&CpuSelections> {
        self.selections.as_ref()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let error_message = self
            .error
            .as_ref()
            .map(|e| text(e).style(text::danger).size(16));

        let title_section = {
            let bold = Font {
                weight: Weight::Bold,
                ..Font::default()
            };
            container(
                text(
                    self.selections
                        .as_ref()
                        .map(|selections| selections.to_string())
                        .unwrap_or(CpuSelections::get_nonselected_string()),
                )
                .font(bold),
            )
            .width(Length::Fill)
            .align_x(Horizontal::Left)
        };

        // TODO: Link the numbers, also, two cores will look weird.
        let controls_height = 16 * 2 + 8 * 2 + 32;
        let controls_section: Element<'_, Message> = if let Some(cpu_selections) = &self.selections
        {
            let mut cpu_checkboxes = column![];
            for cpu_id in 0..self.cpu_count {
                let is_toggled = cpu_selections.get_is_selected(&cpu_id);
                let mut cpu_checkbox = checkbox(is_toggled)
                    .label(format!("CPU {cpu_id}"))
                    .size(16)
                    .text_size(16);
                cpu_checkbox = cpu_checkbox.on_toggle(move |should_activate| Message::Toggle {
                    cpu_id,
                    should_activate,
                });
                cpu_checkboxes = cpu_checkboxes.push(cpu_checkbox);
            }

            scrollable(
                container(cpu_checkboxes.spacing(8).wrap())
                    .align_x(Alignment::Center)
                    .height(controls_height)
                    .padding(8)
                    .style(container::transparent),
            )
            .width(Length::Fill)
            .height(controls_height)
            .auto_scroll(true)
            .direction(scrollable::Direction::Horizontal(Scrollbar::new()))
            .into()
        } else {
            let pulsing_alpha = ((self.progress as f32 / 5.).sin() + 1.) / 2.;
            let ellipses = ".".repeat((self.progress / 5) % 3 + 1);
            container(text(format!("Loading{ellipses}")))
                .width(600)
                .height(controls_height)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center)
                .style(move |_| container::Style {
                    background: Some(Background::Color(
                        Color::BLACK.clone().scale_alpha(pulsing_alpha),
                    )),
                    ..container::Style::default()
                })
                .into()
        };

        column![error_message, title_section, controls_section]
            .width(Length::Fill)
            .spacing(8)
            .align_x(Alignment::Center)
            .into()
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Initialize(cpu_selections) => self.selections = Some(cpu_selections),
            Message::Toggle {
                cpu_id,
                should_activate,
            } => {
                let cpu_selections = unwrap_or!(&mut self.selections, {
                    self.error = Some(String::from("CPU selections not yet initialized."));
                    return;
                });

                if let Err(e) = cpu_selections.toggle_selection(cpu_id, should_activate) {
                    self.error = Some(e.get().to_string());
                }
            }
            Message::Progress => {
                self.progress = self.progress.wrapping_add(1);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Initialize(CpuSelections),
    Progress,
    Toggle {
        cpu_id: usize,
        should_activate: bool,
    },
}

pub fn get_subscriptions() -> Subscription<Message> {
    let progress_period = Duration::from_millis(50);
    iced::time::every(progress_period).map(|_| Message::Progress)
}
