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
            unit: Dimensionless,
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

pub struct ProgressScope<'p, U = Dimensionless> {
    p: &'p mut Progress,
    total: u64,
    done: u64,
    unit: U,
}

impl<'p, U: Unit> ProgressScope<'p, U> {
    pub fn with_unit<U2>(self, unit: U2) -> ProgressScope<'p, U2> {
        ProgressScope {
            p: self.p,
            total: self.total,
            done: self.done,
            unit,
        }
    }

    pub fn inc(&mut self) {
        self.inc_by(1)
    }

    pub fn inc_by(&mut self, by: u64) {
        let prev = self.done;
        self.done += by;

        if self.unit.is_significant_change(prev, self.done) {
            self.do_update()
        }
    }

    fn do_update(&mut self) {
        let &mut Self {
            total,
            done,
            p: Progress { ref title, .. },
            ref unit,
        } = self;

        let done = unit.apply(done);
        let total = unit.apply(total);
        let postfix = unit.postfix_with_leading_space();

        let percent = done * 100 / total;
        let message = format!("{title} {percent}% ({done}/{total}{postfix})");

        self.p.do_update(message)
    }
}

pub trait Unit {
    fn apply(&self, x: u64) -> u64;

    fn postfix_with_leading_space(&self) -> &str;

    fn is_significant_change(&self, prev: u64, next: u64) -> bool;
}

pub struct Dimensionless;

impl Unit for Dimensionless {
    fn apply(&self, x: u64) -> u64 {
        x
    }

    fn postfix_with_leading_space(&self) -> &str {
        ""
    }

    fn is_significant_change(&self, prev: u64, next: u64) -> bool {
        prev < next
    }
}

pub struct KiB;

impl Unit for KiB {
    fn apply(&self, x: u64) -> u64 {
        x / 1024
    }

    fn postfix_with_leading_space(&self) -> &str {
        " KiB"
    }
    fn is_significant_change(&self, prev: u64, next: u64) -> bool {
        prev / 1024 < next / 1024
    }
}
