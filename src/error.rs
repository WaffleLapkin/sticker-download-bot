use std::{fmt::Debug, panic::Location};

use teloxide::RequestError;

pub enum Error<E> {
    Show(E),
    Req(RequestError),
}

impl<E> From<RequestError> for Error<E> {
    fn from(req: RequestError) -> Self {
        Self::Req(req)
    }
}

pub mod callback_query {
    use std::fmt;

    use teloxide::DownloadError;

    use crate::{
        error::{
            downloading::{AlreadyDownloading, SendDocumentError},
            Error,
        },
        query_command::DownloadTarget,
    };

    pub enum CallbackQueryError {
        InvalidButtonData { data: String },
        NoMessage,
        EmptyReply,
        ReplyIsNotSticker,
        AnimatedStickerNotSupported,
        VideoStickerNotSupported,
        AlreadyDownloading(AlreadyDownloading),

        // post errors
        Download(DownloadError),
        SendDocument(SendDocumentError),
    }

    impl CallbackQueryError {
        pub fn is_post(&self) -> bool {
            match self {
                CallbackQueryError::InvalidButtonData { .. }
                | CallbackQueryError::NoMessage
                | CallbackQueryError::EmptyReply
                | CallbackQueryError::ReplyIsNotSticker
                | CallbackQueryError::AnimatedStickerNotSupported
                | CallbackQueryError::VideoStickerNotSupported
                | CallbackQueryError::AlreadyDownloading(_) => false,
                CallbackQueryError::Download(_) | CallbackQueryError::SendDocument(_) => true,
            }
        }
    }

    impl fmt::Display for CallbackQueryError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                CallbackQueryError::InvalidButtonData { data } => {
                    write!(f, "Invalid button data: `{data}`")
                }
                CallbackQueryError::NoMessage => write!(f, "No message? :c"),
                CallbackQueryError::EmptyReply => write!(f, "Reply is empty"),
                CallbackQueryError::ReplyIsNotSticker => write!(f, "Reply is not a sticker"),
                CallbackQueryError::AnimatedStickerNotSupported => {
                    write!(f, "Animated stickers are not yet supported")
                }
                CallbackQueryError::VideoStickerNotSupported => {
                    write!(f, "Video stickers are not yet supported")
                }
                CallbackQueryError::AlreadyDownloading(AlreadyDownloading(target)) => {
                    let what = match target {
                        DownloadTarget::Single => "sticker",
                        DownloadTarget::All => "set",
                    };

                    write!(f, "This {what} is already being downloaded")
                }
                CallbackQueryError::Download(err) => {
                    // FIXME: determine (s)
                    write!(f, "An error happened while downloading sticker(s): <code>{err}</code> :(\n\nTry again later.")
                }
                CallbackQueryError::SendDocument(SendDocumentError(e)) => {
                    write!(f, "Couldn't send the document: {e}.\n Try again later.")
                }
            }
        }
    }

    impl From<AlreadyDownloading> for Error<CallbackQueryError> {
        fn from(ad: AlreadyDownloading) -> Self {
            Error::Show(CallbackQueryError::AlreadyDownloading(ad))
        }
    }
    impl From<DownloadError> for Error<CallbackQueryError> {
        fn from(d: DownloadError) -> Self {
            Error::Show(CallbackQueryError::Download(d))
        }
    }
    impl From<SendDocumentError> for Error<CallbackQueryError> {
        fn from(sd: SendDocumentError) -> Self {
            Error::Show(CallbackQueryError::SendDocument(sd))
        }
    }

    pub fn invalid_button_data(data: &str) -> Result<(), Error<CallbackQueryError>> {
        let data = data.to_owned();
        Err(Error::Show(CallbackQueryError::InvalidButtonData { data }))
    }

    pub fn no_message() -> Error<CallbackQueryError> {
        Error::Show(CallbackQueryError::NoMessage)
    }

    pub fn empty_reply() -> Error<CallbackQueryError> {
        Error::Show(CallbackQueryError::EmptyReply)
    }

    pub fn reply_is_not_sticker() -> Error<CallbackQueryError> {
        Error::Show(CallbackQueryError::ReplyIsNotSticker)
    }

    pub fn animated_sticker_not_supported() -> Error<CallbackQueryError> {
        Error::Show(CallbackQueryError::AnimatedStickerNotSupported)
    }

    pub fn video_sticker_not_supported() -> Error<CallbackQueryError> {
        Error::Show(CallbackQueryError::VideoStickerNotSupported)
    }
}

pub mod downloading {
    use teloxide::RequestError;

    use crate::query_command::DownloadTarget;

    pub struct SendDocumentError(pub RequestError);

    pub struct AlreadyDownloading(pub DownloadTarget);
}

pub trait ResultExt {
    type Item;
    type Err;

    fn fine(self)
    where
        Self::Err: Debug;
}

impl<T, E> ResultExt for Result<T, E> {
    type Item = T;

    type Err = E;

    #[track_caller]
    fn fine(self)
    where
        <Self as ResultExt>::Err: Debug,
    {
        let loc = Location::caller();

        if let Err(err) = self {
            log::error!("Ignoring error @ {loc}: {err:?}");
        }
    }
}
