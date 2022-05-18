use std::{
    collections::HashSet,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
    task::Poll,
    time::Instant,
};

use bytes::Bytes;
use emojis::Emoji;
use futures::{stream, Stream, StreamExt, TryStreamExt};
use teloxide::{
    net::Download,
    prelude::Requester,
    types::{ChatId, Sticker},
};
use tokio::io::AsyncWriteExt;
use unicode_segmentation::UnicodeSegmentation;
use uuid::Uuid;

use crate::query_command::{DownloadFormat, DownloadTarget};

#[derive(Clone)]
pub struct Downloader {
    bot: crate::Bot,
    in_flight: Arc<Mutex<HashSet<i32>>>,
}

pub struct AlreadyDownloading;

pub struct Tasks {
    pub message_id: i32,
    //pub chat_id: ChatId,
    //pub target: DownloadTarget,
    pub format: DownloadFormat,
    pub stickers: Vec<Task>,
}

pub struct Task {
    pub path: String,
    pub name: String,
}

type Item = (
    String,
    Result<Vec<Bytes>, <crate::Bot as Download<'static>>::StreamErr>,
);

impl Downloader {
    pub fn new(bot: crate::Bot) -> Self {
        Self {
            bot,
            in_flight: <_>::default(),
        }
    }

    // TODO: progress
    pub fn download(&self, t: Tasks) -> Result<impl Stream<Item = Item>, AlreadyDownloading> {
        if !self.in_flight.lock().unwrap().insert(t.message_id) {
            return Err(AlreadyDownloading);
        }

        assert_eq!(t.format, DownloadFormat::Webp);
        let format = t.format;

        let Self { bot, in_flight } = self.clone();

        let stream = stream::iter(t.stickers)
            .map(move |Task { path, name }| {
                let bot = bot.clone();
                async move {
                    let file_name = format!("{name}.{ext}", ext = format.ext());
                    let bytes: Result<Vec<_>, _> =
                        bot.download_file_stream(&path).try_collect().await;

                    (file_name, bytes)
                }
            })
            .buffer_unordered(C);

        let stream = defer_stream(stream, move || {
            in_flight.lock().unwrap().remove(&t.message_id);
        });
        Ok(stream)
    }
}

/// How many files should be downloaded concurrently at a time.
///
/// I've ""benched"" the download code by hand using `Instent::now()`/`.elapsed()`
/// running with each `C` 3 times on the same 120-sticker sticker pack.
///
/// After `C = 8` the change in speed is quite small, so I've decided to keep `C = 8`.
///
/// - Is this noisy as hell? Yes
/// - Is this a good benchmark? No, of course no
/// - Is this a good enough benchmark for this case? Probably
/// - Why Run-B is almost always faster that A and C? I have no idea
///
/// Here is a graph:
///
/// ![graph](https://media.discordapp.net/attachments/868574040032428082/976095018269933638/2022-05-17_16-12.png?width=1440&height=402)
///
/// Here is the data in `csv` format:
/// ```csv
/// Number of concurrent tasks,Run-A,Run-B,Run-C
/// 1,10.559181064,10.359101701,11.612704131
/// 2,5.480041006,6.448020875,5.009588568
/// 3,3.972163538,3.904767156,3.71170429
/// 4,2.662560592,3.040363638,2.683632964
/// 5,2.360479824,2.301494778,2.165248723
/// 6,1.82522527,1.882500603,1.950526216
/// 7,1.57640211,1.596788356,1.70625803
/// 8,1.584204552,1.531547792,1.486753116
/// 9,1.473959986,1.305716177,1.316937982
/// 10,1.393999577,1.167951831,1.210302591
/// 11,1.173377195,0.965236589,1.066991419
/// 12,1.125328165,1.001481012,1.18970862
/// 13,1.115044576,0.9110424500000001,0.935644217
/// 14,1.150154713,0.904767917,0.8803149969999999
/// 15,1.072681067,0.848284024,0.8159171669999999
/// 16,1.068943187,0.8615857610000001,0.9038348580000001
/// 17,1.016442789,0.818071889,0.81563021
/// 18,0.8817281,0.689204445,0.7200475300000001
/// 19,0.9856043250000001,0.776953419,0.785015845
/// 20,0.925884999,0.6747590800000001,0.694237659
/// 21,0.861050254,0.624375912,0.650668464
/// 22,0.852608532,0.60369127,0.6289302259999999
/// 23,0.924439437,0.7476137,0.6300453149999999
/// 24,0.864900087,0.6085802159999999,0.6431513179999999
/// 25,0.882933488,0.857488376,0.605686578
/// 26,0.756153768,0.617684287,0.568035089
/// 27,0.731639725,0.512946003,0.665593856
/// 28,0.866960764,0.640780345,0.643327848
/// 29,0.723518494,0.5759247820000001,0.606090639
/// 30,0.7879472279999999,0.476932551,0.493704856
/// 31,0.8705269080000001,0.547131948,0.574714792
/// 32,0.74525418,0.608744778,0.541783306
/// 40,0.8804177120000001,0.513784259,0.552798597
/// 48,0.686019376,0.628310873,0.498781803
/// 56,0.73908673,0.421861397,0.429524717
/// 64,0.7969362,0.494137073,0.385381665
/// 120,0.6980425659999999,0.313278465,0.30435578900000004
/// ```
const C: usize = 8;

/// A hacky way to run something on drop of a stream
fn defer_stream<S: Stream>(stream: S, f: impl FnOnce()) -> impl Stream<Item = S::Item> {
    #[pin_project::pin_project(PinnedDrop)]
    struct DeferStream<S, F: FnOnce()> {
        #[pin]
        stream: S,
        f: Option<F>,
    }

    #[pin_project::pinned_drop]
    impl<S, F: FnOnce()> PinnedDrop for DeferStream<S, F> {
        fn drop(self: Pin<&mut Self>) {
            if let Some(f) = self.project().f.take() {
                f()
            }
        }
    }

    impl<S: Stream, F: FnOnce()> Stream for DeferStream<S, F> {
        type Item = S::Item;

        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            self.project().stream.poll_next(cx)
        }
    }

    DeferStream { stream, f: Some(f) }
}
