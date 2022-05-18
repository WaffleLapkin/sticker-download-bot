// Status:
// - Basic functionality (downloading sticker packs) works!
// - For some reason the bot is slow (need to check why)
// - Converting to .png is NOT implemented
// - Messages/interface are very much work in progress
// - Progress is not shown
// - The code is quite bad in some places/32 warnings/wip
// - Sticker pack information is not provided (archive should include .json)

mod download;
mod error;
mod query_command;

use std::{collections::VecDeque, io::Write, iter::Peekable, pin::Pin, task, time::Duration};

use bytes::Bytes;
use emojis::Emoji;
use futures::{stream, StreamExt, TryStreamExt};
use teloxide::{
    adaptors::{DefaultParseMode, Throttle},
    dispatching::{update_listeners::polling, HandlerExt, MessageFilterExt, UpdateHandler},
    dptree::{self, deps, Handler},
    payloads::{AnswerCallbackQuerySetters, EditMessageTextSetters, SendMessageSetters},
    prelude::{AutoSend, Dispatcher, RequesterExt},
    respond,
    types::{CallbackQuery, ChatAction::UploadDocument, InlineQuery, InputFile, ParseMode, Update},
    utils::command::parse_command,
    RequestError,
};
use teloxide::{
    dispatching::UpdateFilterExt,
    payloads::setters::*,
    prelude::Requester,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Me, Message, Sticker},
};
use tokio::{io::AsyncRead, task::spawn_blocking};
use unicode_segmentation::UnicodeSegmentation;
use zip::{write::FileOptions, ZipWriter};

use crate::{
    download::{AlreadyDownloading, Downloader, Task, Tasks},
    error::{callback_query::CallbackQueryError, Error},
    query_command::{ActionDownload, DownloadFormat, DownloadTarget, QueryAction, QueryCommand},
};

type Bot = AutoSend<DefaultParseMode<Throttle<teloxide::Bot>>>;

fn main() {
    pretty_env_logger::init();

    let test = true;

    // Using single-thread runtime is not really needed, I could use multi-thread runtime here.
    // However, since I don't expect this bot to be used much, I can save some VPS resources (?probably).
    //
    // I don't use `#[tokio::main]` to reduce macros & magic used and speedup compilation a little bit
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let _rt_guard = rt.enter();

    let bot = teloxide::Bot::from_env()
        // This will protect the bot from Telegram limits, if it ever reaches them
        .throttle(<_>::default())
        // Set default parse mode
        .parse_mode(ParseMode::Html)
        // Allow using `.await` without `.send()` or requests
        .auto_send();

    let mut dp = Dispatcher::builder(bot.clone(), dispatch_tree())
        .dependencies(deps![Downloader::new(bot.clone())])
        .build();
    dp.setup_ctrlc_handler();

    if test {
        let listener = polling(bot, Some(std::time::Duration::from_secs(1)), None, None);

        rt.block_on(async {
            dp.dispatch_with_listener(
                listener,
                teloxide::error_handlers::LoggingErrorHandler::new(),
            )
            .await
        });

        return;
    }

    rt.block_on(async { dp.dispatch().await });
}

fn dispatch_tree() -> UpdateHandler<RequestError> {
    dptree::entry()
        .branch(
            Update::filter_message()
                .branch(Message::filter_sticker().endpoint(sticker))
                .branch(Message::filter_text().endpoint(text)),
        )
        .branch(Update::filter_callback_query().endpoint(callback_query))
}

async fn sticker(bot: Bot, sticker: Sticker, message: Message) -> Result<(), RequestError> {
    let download_png = InlineKeyboardButton::callback(
        "Download sticker as .png",
        QueryCommand::download(DownloadTarget::Single, DownloadFormat::Png).encode(),
    );
    let download_webp = InlineKeyboardButton::callback(
        "Download sticker as .webp",
        QueryCommand::download(DownloadTarget::Single, DownloadFormat::Webp).encode(),
    );
    let download_png_set = InlineKeyboardButton::callback(
        "Download set as .png",
        QueryCommand::download(DownloadTarget::All, DownloadFormat::Png).encode(),
    );
    let download_webp_set = InlineKeyboardButton::callback(
        "Download set as .webp",
        QueryCommand::download(DownloadTarget::All, DownloadFormat::Webp).encode(),
    );

    let kb = InlineKeyboardMarkup::new([
        [download_png],
        [download_webp],
        [download_png_set],
        [download_webp_set],
    ]);

    bot.send_message(message.chat.id, "TODO")
        .reply_markup(kb)
        .reply_to_message_id(message.id)
        .await?;

    Ok(())
}

async fn text(bot: Bot, text: String, message: Message, me: Me) -> Result<(), RequestError> {
    let chat_id = message.chat.id;

    // We could use teloxide derive macros for commands, but for just /start and /help that's a bit of an overkill.
    if let Some((command, _args)) = parse_command(&text, me.username()) {
        match command {
            "start" => {
                bot.send_message(chat_id, format!("start (TODO)")).await?;
            }
            "help" => {
                bot.send_message(chat_id, format!("help (TODO)")).await?;
            }
            _ => {
                bot.send_message(
                    chat_id,
                    format!(
                        "Unknown command `{command}`, see /help for the list of available commands"
                    ),
                )
                .await?;
            }
        }

        return Ok(());
    }

    bot.send_message(
        chat_id,
        format!(
            "Use /help for the list of available commands and instructions on how to use the bot"
        ),
    )
    .await?;

    Ok(())
}

async fn callback_query(bot: Bot, query: CallbackQuery, d: Downloader) -> Result<(), RequestError> {
    match callback_query_inner(&bot, &query, d).await {
        Ok(()) => Ok(()),
        Err(Error::Req(e)) => Err(e),
        Err(Error::Show(e)) => {
            bot.answer_callback_query(query.id)
                .text(format!("Error: {e}"))
                .show_alert(true)
                .await?;

            Ok(())
        }
    }
}

async fn callback_query_inner(
    bot: &Bot,
    query: &CallbackQuery,
    d: Downloader,
) -> Result<(), Error<CallbackQueryError>> {
    use error::callback_query as err;

    let data = match &query.data {
        Some(data) => data,
        None => {
            return Ok(());
        }
    };

    let command = match QueryCommand::decode(data) {
        Some(c) => c,
        None => return err::invalid_button_data(data),
    };

    let QueryAction::Download(action) = command.action;
    callback_query_download(bot, action, query, d).await?;

    Ok(())
}

async fn callback_query_download(
    bot: &Bot,
    action: ActionDownload,
    query: &CallbackQuery,
    d: Downloader,
) -> Result<(), Error<CallbackQueryError>> {
    use error::callback_query as err;

    let message = match &query.message {
        Some(m) => m,
        None => return err::no_message(),
    };

    let reply = match message.reply_to_message() {
        Some(r) => r,
        None => return err::empty_reply(),
    };

    let sticker = match reply.sticker() {
        Some(s) => s,
        None => return err::reply_is_not_sticker(),
    };

    bot.edit_message_text(message.chat.id, message.id, "Queueing download request...")
        .await?;

    let sticker_set_name = sticker.set_name.clone();
    let tasks = prepare_download_tasks(bot, message.id, sticker, action).await?;
    match d.download(tasks) {
        Ok(stream) => {
            bot.answer_callback_query(&query.id).await?;

            let bot = bot.clone();
            let chat_id = message.chat.id;
            let message_id = message.id;
            tokio::spawn(async move {
                let res: Result<Vec<_>, _> = stream
                    .map(|(file_name, res)| res.map(|v| (file_name, v)))
                    .try_collect()
                    .await;

                // FIXME: handle errors

                match res {
                    Ok(mut stickers) if stickers.len() == 1 => {
                        let (name, bytes) = stickers.pop().unwrap();
                        let file = InputFile::read(chunked_read(bytes)).file_name(name); // FIXME: should be something like chunked()
                        bot.send_document(chat_id, file).await;
                        bot.delete_message(chat_id, message_id).await;
                    }
                    Ok(stickers) => {
                        bot.send_chat_action(chat_id, UploadDocument).await;

                        // FIXME: is this spawn_blocking needed? can we stream the zip?
                        let zip = spawn_blocking(|| {
                            let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::with_capacity(
                                0, /* FIXME */
                            )));

                            for (name, bytes) in stickers {
                                zip.start_file(name, FileOptions::default() /* FIXME */);
                                for b in bytes {
                                    zip.write_all(&b);
                                }
                            }

                            zip.finish().unwrap().into_inner()
                        })
                        .await
                        .unwrap();

                        let archive_name =
                            format!("{}.zip", sticker_set_name.as_deref().unwrap_or("stickers"));
                        let file = InputFile::memory(zip).file_name(archive_name);
                        bot.send_document(chat_id, file).await;
                        bot.delete_message(chat_id, message_id).await;
                    }
                    Err(err) => {
                        let text = format!("An error happened while downloading sticker(s): <code>{err}</code> :(\n\nTry again later."); // FIXME: determine (s)
                        bot.edit_message_text(chat_id, message_id, text).await;
                    }
                }
            });
        }

        // FIXME use ? or smh
        Err(AlreadyDownloading) => {
            let what = match action.target {
                DownloadTarget::Single => "sticker",
                DownloadTarget::All => "set",
            };

            bot.answer_callback_query(&query.id)
                .text(format!("Error: This {what} is already downloading"))
                .await?;
        }
    };

    Ok(())
}

async fn prepare_download_tasks(
    bot: &Bot,
    message_id: i32,
    sticker: &Sticker,
    ActionDownload { target, format }: ActionDownload,
) -> Result<Tasks, Error<CallbackQueryError>> {
    let set = match &sticker.set_name {
        Some(name) => Some(bot.get_sticker_set(name).await?),
        None => None,
    };

    let named_and_identified = match (target, set) {
        (DownloadTarget::Single, set) | (DownloadTarget::All, set @ None) => {
            let idx = set.and_then(|set| {
                set.stickers
                    .iter()
                    .enumerate()
                    .find(|(_, s)| s.file_unique_id == sticker.file_unique_id)
                    .map(|(i, _)| i as u8)
            });

            vec![(
                sticker_name(idx, sticker.emoji.as_deref().unwrap_or_default()),
                sticker.file_id.clone(),
            )]
        }
        (DownloadTarget::All, Some(set)) => set
            .stickers
            .into_iter()
            .enumerate()
            .map(|(idx, s)| {
                (
                    sticker_name(Some(idx as u8), s.emoji.as_deref().unwrap_or_default()),
                    s.file_id,
                )
            })
            .collect(),
    };

    let stickers = stream::iter(named_and_identified)
        .map(|(name, file_id)| async {
            bot.get_file(file_id).await.map(|f| Task {
                path: f.file_path,
                name,
            })
        })
        .buffer_unordered(16 /* FIXME: choose constant */)
        .try_collect()
        .await?;

    Ok(Tasks {
        message_id,
        format,
        stickers,
    })
}

fn sticker_name(idx: Option<u8>, emojis: &str) -> String {
    let name = emojis
        .graphemes(true)
        .flat_map(|cluster| emojis::get(cluster))
        .map(Emoji::name)
        .next()
        .unwrap_or_else(|| /* FIXME: warn */ "malformed_emoji")
        .replace(' ', "_");

    match idx {
        Some(idx) => format!("{idx:03}_{name}"),
        None => name,
    }
}

fn chunked_read(bytes: Vec<Bytes>) -> ChunkedRead {
    ChunkedRead {
        bytes: bytes.into(),
    }
}
struct ChunkedRead {
    bytes: VecDeque<Bytes>,
}

impl AsyncRead for ChunkedRead {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> task::Poll<std::io::Result<()>> {
        let ready = task::Poll::Ready(Ok(()));

        let len = match self.bytes.front() {
            None => return ready,
            Some(cur) => cur.len(),
        };

        let bytes = match () {
            _ if len <= buf.remaining() => self.bytes.pop_front().unwrap(),
            _ => self.bytes.front_mut().unwrap().split_to(buf.remaining()),
        };

        buf.put_slice(&bytes);

        ready
    }
}
