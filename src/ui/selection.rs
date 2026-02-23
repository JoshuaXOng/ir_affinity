use iced::Alignment;
use iced::Element;
use iced::Font;
use iced::Length;
use iced::Subscription;
use iced::alignment::Horizontal;
use iced::font::Weight;
use iced::widget::scrollable::Scrollbar;
use iced::widget::{checkbox, column, container, scrollable, text};
use crate::persistence::CpuSelections;

pub struct CpuSelection {
    inner: CpuSelections,
    progress: usize,
    error: Option<String>,
}

impl CpuSelection {
    pub fn new(cpu_selections: CpuSelections) -> Self {
        Self {
            inner: cpu_selections,
            progress: 0,
            error: None,
        }
    }
}

impl CpuSelection {
    pub fn get_inner(&self) -> &CpuSelections {
        &self.inner
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
            container(text(self.inner.to_string()).font(bold))
                .width(Length::Fill)
                .align_x(Horizontal::Left)
        };

        // TODO: Link the numbers, also, two cores will look weird, etc.
        let controls_height = 16 * 2 + 8 * 2 + 32;
        let controls_section = {
            let mut cpu_checkboxes = column![];
            for cpu_id in 0..self.inner.get_cpu_count() {
                let is_toggled = self.inner.get_is_selected(&cpu_id);
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
        };

        column![error_message, title_section, controls_section]
            .width(Length::Fill)
            .spacing(8)
            .align_x(Alignment::Center)
            .into()
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Toggle {
                cpu_id,
                should_activate,
            } => {
                if let Err(e) = self.inner.toggle_selection(cpu_id, should_activate) {
                    self.error = Some(e.get().to_string());
                }
            }
            Message::Progress => self.progress = self.progress.wrapping_add(1),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Toggle {
        cpu_id: usize,
        should_activate: bool,
    },
    Progress,
}

const PROGRESS_COOLDOWN_MILLISECONDS: u64 = 50;

pub fn get_subscriptions() -> Subscription<Message> {
    let progress_period = std::time::Duration::from_millis(PROGRESS_COOLDOWN_MILLISECONDS);
    iced::time::every(progress_period).map(|_| Message::Progress)
}
