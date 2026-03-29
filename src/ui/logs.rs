use iced::{
    Alignment, Element, Length,
    widget::{column, container, rule, scrollable, text},
};

#[derive(Debug, Clone)]
pub struct MessageLog {
    inner: Vec<String>,
}

impl MessageLog {
    pub fn new() -> Self {
        Self {
            inner: vec![
                String::from("This is the message log."),
                String::from("Hello There!"),
            ],
        }
    }
}

impl MessageLog {
    pub fn view(&self) -> Element<'_, Message> {
        let mut message_logs = column![];
        for (message_index, message_log) in self.inner.iter().enumerate() {
            message_logs = message_logs.push(text(message_log));
            let is_last = message_index == self.inner.len() - 1;
            if !is_last {
                message_logs = message_logs.push(rule::horizontal(1));
            }
        }

        let logs_height = 85;
        scrollable(
            container(message_logs.spacing(8).wrap())
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .padding(8)
                .style(container::secondary),
        )
        .width(Length::Fill)
        .height(logs_height)
        .auto_scroll(true)
        .into()
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Append(message) => self.inner.insert(0, message),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Append(String),
}
