use std::io;

use fast_image_resize::{Image, ImageView, MulDiv, PixelType, ResizeAlg, Resizer};
use teloxide::types::InputFile;

/// Generates a thumbnail for a sticker archive given a webp image.
pub fn generate_thumbnail(webp: &[u8]) -> InputFile {
    // FIXME: remove unwraps

    let (w, h, raw) = libwebp::WebPDecodeRGBA(webp).unwrap();

    let mut src = ImageView::from_buffer(
        w.try_into().unwrap(),
        h.try_into().unwrap(),
        &raw,
        PixelType::U8x4,
    )
    .unwrap();

    // Desired size
    let w = 256.try_into().unwrap();
    let h = w;

    // Set cropping so the result is not squished
    src.set_crop_box_to_fit_dst_size(w, h, None);

    // Resized to desired size
    let resized = {
        let mut dst = Image::new(w, h, PixelType::U8x4);

        let mut resizer = Resizer::new(ResizeAlg::Nearest);
        resizer.resize(&src, &mut dst.view_mut()).unwrap();

        dst
    };

    // With color channels multiplied by alpha (the result will be that transparent pixel are black)
    //
    // This is needed
    let no_alpha = {
        let mut dst = Image::new(w, h, PixelType::U8x4);

        let mul_div = MulDiv::default();
        mul_div
            .multiply_alpha(&resized.view(), &mut dst.view_mut())
            .unwrap();

        dst
    };

    // Converted to jpeg
    let compressed = {
        let mut dst = io::Cursor::new(Vec::new());

        let encoder = jpeg_encoder::Encoder::new(&mut dst, 90);
        encoder
            .encode(&no_alpha.buffer(), 256, 256, jpeg_encoder::ColorType::Rgba)
            .unwrap();

        dst.into_inner()
    };

    InputFile::memory(compressed)
}
