use futures::future::FutureExt;
use std::pin::Pin;
use teloxide::{prelude::Requester, types::ChatId};
use tokio::task::JoinHandle;

use crate::{error::ResultExt, Bot};

pub struct Progress {
    title: String,
    bot: Bot,
    chat_id: ChatId,
    message_id: i32,
    task: Option<JoinHandle<()>>,
}

impl Progress {
    pub fn new(bot: &Bot, title: &str, chat_id: ChatId, message_id: i32) -> Self {
        let mut this = Self {
            title: title.to_owned(),
            bot: bot.clone(),
            chat_id,
            message_id,
            task: None,
        };

        this.do_update(this.title.clone());
        this
    }

    pub fn scope(&mut self, title: &str, total: u64) -> ProgressScope<'_> {
        self.title = title.to_owned();
        ProgressScope {
            p: self,
            total,
            done: 0,
        }
    }

    #[allow(dead_code)]
    pub fn title(&mut self, title: &str) {
        self.title = title.to_owned();
        self.do_update(self.title.clone())
    }

    pub fn title_imp(&mut self, title: &str) {
        self.title = title.to_owned();
        self.do_update_imp(self.title.clone())
    }

    fn do_update(&mut self, to: String) {
        if let Some(jh) = self.task.as_mut() {
            if Pin::new(jh).now_or_never().is_some() {
                self.task = None;
            }
        }

        self.task.get_or_insert_with(|| {
            let bot = self.bot.clone();
            let &mut Self {
                chat_id,
                message_id,
                ..
            } = self;

            tokio::spawn(async move { bot.edit_message_text(chat_id, message_id, to).await.fine() })
        });
    }

    fn do_update_imp(&mut self, to: String) {
        let task = self.task.take();

        let bot = self.bot.clone();
        let &mut Self {
            chat_id,
            message_id,
            ..
        } = self;

        let handle = tokio::spawn(async move {
            if let Some(task) = task {
                task.await.fine();
            }

            bot.edit_message_text(chat_id, message_id, to).await.fine()
        });

        self.task = Some(handle);
    }
}

pub struct ProgressScope<'p> {
    p: &'p mut Progress,
    total: u64,
    done: u64,
}

impl ProgressScope<'_> {
    pub fn inc(&mut self) {
        self.inc_by(1)
    }

    pub fn inc_by(&mut self, by: u64) {
        self.done += by;
        self.do_update()
    }

    fn do_update(&mut self) {
        let &mut Self {
            total,
            done,
            p: Progress { ref title, .. },
        } = self;

        let percent = done * 100 / total;
        let message = format!("{title} {percent}% ({done}/{total})");

        self.p.do_update(message)
    }
}
