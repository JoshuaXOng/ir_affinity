use iced::{
    Alignment, Element, Font, Length, Task, font::Weight, widget::{column, scrollable, text}
};

use crate::ui::{INITIAL_WINDOW_SIZE, IS_WINDOW_RESIZABLE, MAIN_WINDOW_NAME};

pub fn run_error_ui(error: String) -> iced::Result {
    iced::application(
        move || IrAffinity {
            error: error.clone(),
        },
        IrAffinity::update,
        IrAffinity::view,
    )
    .title(MAIN_WINDOW_NAME)
    .window_size(INITIAL_WINDOW_SIZE)
    .resizable(IS_WINDOW_RESIZABLE)
    .run()
}

struct IrAffinity {
    error: String,
}

impl IrAffinity {
    fn view(&self) -> Element<'_, Message> {
        let bold = Font {
            weight: Weight::Bold,
            ..Font::default()
        };
        scrollable(
            column![
                text("FAILED TO INITIALIZE!!!").font(bold),
                text(&self.error).style(text::danger)
            ]
            .width(Length::Fill)
            .spacing(16)
            .padding(16)
            .align_x(Alignment::Center),
        )
        .into()
    }

    fn update(&mut self, _message: Message) -> Task<Message> {
        Task::none()
    }
}

#[derive(Debug, Clone)]
enum Message {}
