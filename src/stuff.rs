//! Random stuff lives here.

use std::{collections::VecDeque, io::Write, pin::Pin, task};

use bytes::Bytes;
use emojis::Emoji;
use teloxide::types::InputFile;
use tokio::io::AsyncRead;
use unicode_segmentation::UnicodeSegmentation;
use zip::{result::ZipError, write::FileOptions, ZipWriter};

/// Archive files together making a `.zip` file.
///
/// Files are represented by a `(name, bytes)` tuple.
pub fn archive(name: &str, files: Vec<(String, Vec<Bytes>)>) -> Result<InputFile, ZipError> {
    let zip = {
        // Technically this does a blocking write.
        // But since it writes to memory and does not do compression, it takes negligible time (around 5ms).
        // So it doesn't seem to make sense to use `spawn_blocking` here.

        let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::with_capacity(0 /* FIXME */)));

        let options = FileOptions::default()
            // Compressing images is pointless because they are already compressed.
            //
            // From my non-exhaustive testing using default `deflate` compression
            // makes archiving 29 times slower while making the resulting .zip a little bit bigger.
            .compression_method(zip::CompressionMethod::Stored);

        for (name, bytes) in files {
            zip.start_file(name, options)?;
            for b in bytes {
                zip.write_all(&b)?;
            }
        }

        zip.finish()?.into_inner()
    };

    let archive_name = format!("{}.zip", name);
    let file = InputFile::memory(zip).file_name(archive_name);

    Ok(file)
}

/// Returns a file name for a sticker that is `idx`-th in its sticker pack (`None` if it isn't in any) given its associated `emojis`.
pub fn sticker_name(idx: Option<u8>, emojis: &str) -> String {
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

/// Returns `impl AsyncRead` that returns all bytes from each chunk in order.
pub fn chunked_read(chunks: Vec<Bytes>) -> impl AsyncRead {
    ChunkedRead {
        bytes: chunks.into(),
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
