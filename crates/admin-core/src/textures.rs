//! Skins and capes. An uploaded image is untrusted: its dimensions are
//! checked before the pixels are decoded, and it is re-encoded so that
//! nothing but the pixels reaches other players' clients.

use std::io::Cursor;

use png::{BitDepth, ColorType, Decoder, Encoder, Limits, Transformations};

use crate::error::{Error, Result};

/// Largest upload accepted, in bytes.
pub const MAX_UPLOAD: usize = 1024 * 1024;
/// Largest texture edge, in pixels: an 8x "HD" skin.
const MAX_EDGE: u32 = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureKind {
    Skin,
    Cape,
}

impl TextureKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "skin" => Some(TextureKind::Skin),
            "cape" => Some(TextureKind::Cape),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TextureKind::Skin => "skin",
            TextureKind::Cape => "cape",
        }
    }
}

/// Size of the canvas the texture is stored on, or `None` when the
/// dimensions are not ones the game can use.
fn canvas(kind: TextureKind, width: u32, height: u32) -> Option<(u32, u32)> {
    if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
        return None;
    }
    let wide = width.is_multiple_of(64) && height * 2 == width;
    match kind {
        TextureKind::Skin => {
            (wide || (width.is_multiple_of(64) && height == width)).then_some((width, height))
        }
        TextureKind::Cape if wide => Some((width, height)),
        // The 22x17 layout many cape editors export; the game wants it on a
        // 64x32 canvas.
        TextureKind::Cape => {
            let scale = width / 22;
            (width.is_multiple_of(22) && height == scale * 17).then_some((scale * 64, scale * 32))
        }
    }
}

fn bad_image(err: impl std::fmt::Display) -> Error {
    Error::invalid(format!("not a usable PNG image: {err}"))
}

/// Validates an uploaded texture and returns it as a clean RGBA PNG.
/// CPU-bound; call it off the async threads.
pub fn sanitize(kind: TextureKind, upload: &[u8]) -> Result<Vec<u8>> {
    if upload.len() > MAX_UPLOAD {
        return Err(Error::invalid("the image is larger than 1 MB"));
    }
    let mut decoder = Decoder::new_with_limits(
        Cursor::new(upload),
        Limits {
            bytes: 16 * 1024 * 1024,
        },
    );
    decoder.set_transformations(
        Transformations::EXPAND | Transformations::STRIP_16 | Transformations::ALPHA,
    );
    let header = decoder.read_header_info().map_err(bad_image)?;
    let (width, height) = (header.width, header.height);
    let (canvas_width, canvas_height) = canvas(kind, width, height).ok_or_else(|| {
        Error::invalid(match kind {
            TextureKind::Skin => "a skin must be 64x64 or 64x32 pixels (or a multiple, up to 512)",
            TextureKind::Cape => "a cape must be 64x32 or 22x17 pixels (or a multiple)",
        })
    })?;

    let mut reader = decoder.read_info().map_err(bad_image)?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| bad_image("too large"))?;
    let mut raw = vec![0u8; size];
    let frame = reader.next_frame(&mut raw).map_err(bad_image)?;
    if frame.bit_depth != BitDepth::Eight || frame.width != width || frame.height != height {
        return Err(bad_image("unsupported pixel format"));
    }
    raw.truncate(frame.buffer_size());
    let pixels: Vec<[u8; 4]> = match frame.color_type {
        ColorType::Rgba => raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| [p[0], p[1], p[2], p[3]])
            .collect(),
        ColorType::Rgb => raw
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        ColorType::GrayscaleAlpha => raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        ColorType::Grayscale => raw.iter().map(|&v| [v, v, v, 255]).collect(),
        ColorType::Indexed => return Err(bad_image("unsupported pixel format")),
    };
    if pixels.len() != (width as usize) * (height as usize) {
        return Err(bad_image("truncated pixel data"));
    }

    // Transparent canvas, image in the top-left corner.
    let mut out = vec![0u8; (canvas_width as usize) * (canvas_height as usize) * 4];
    for (y, row) in pixels.chunks_exact(width as usize).enumerate() {
        let start = y * (canvas_width as usize) * 4;
        for (x, pixel) in row.iter().enumerate() {
            out[start + x * 4..start + x * 4 + 4].copy_from_slice(pixel);
        }
    }

    let mut encoded = Vec::new();
    let mut encoder = Encoder::new(&mut encoded, canvas_width, canvas_height);
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(bad_image)?;
    writer.write_image_data(&out).map_err(bad_image)?;
    writer.finish().map_err(bad_image)?;
    Ok(encoded)
}

#[cfg(test)]
pub(crate) fn test_png(width: u32, height: u32, color: ColorType) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, width, height);
    encoder.set_color(color);
    encoder.set_depth(BitDepth::Eight);
    encoder
        .add_text_chunk("Comment".into(), "secret metadata".into())
        .unwrap();
    let mut writer = encoder.write_header().unwrap();
    let data = vec![200u8; (width * height) as usize * color.samples()];
    writer.write_image_data(&data).unwrap();
    writer.finish().unwrap();
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dimensions(png: &[u8]) -> (u32, u32) {
        let mut decoder = Decoder::new(Cursor::new(png));
        let info = decoder.read_header_info().unwrap();
        (info.width, info.height)
    }

    #[test]
    fn skins_are_checked_and_reencoded() {
        let upload = test_png(64, 64, ColorType::Rgb);
        let clean = sanitize(TextureKind::Skin, &upload).unwrap();
        assert_eq!(dimensions(&clean), (64, 64));
        // Text chunks do not survive.
        assert!(!clean.windows(6).any(|w| w == b"secret"));
        // Sanitising is stable, so the same skin always has the same hash.
        assert_eq!(sanitize(TextureKind::Skin, &clean).unwrap(), clean);

        assert!(sanitize(TextureKind::Skin, &test_png(64, 32, ColorType::Rgba)).is_ok());
        assert!(sanitize(TextureKind::Skin, &test_png(128, 128, ColorType::Rgba)).is_ok());
        assert!(sanitize(TextureKind::Skin, &test_png(64, 48, ColorType::Rgba)).is_err());
        assert!(sanitize(TextureKind::Skin, &test_png(100, 100, ColorType::Rgba)).is_err());
        assert!(
            sanitize(
                TextureKind::Skin,
                &test_png(1024, 1024, ColorType::Grayscale)
            )
            .is_err()
        );
        assert!(sanitize(TextureKind::Skin, b"not a png").is_err());
    }

    #[test]
    fn small_capes_are_padded() {
        let clean = sanitize(TextureKind::Cape, &test_png(22, 17, ColorType::Rgba)).unwrap();
        assert_eq!(dimensions(&clean), (64, 32));
        let clean = sanitize(TextureKind::Cape, &test_png(44, 34, ColorType::Rgba)).unwrap();
        assert_eq!(dimensions(&clean), (128, 64));
        assert!(sanitize(TextureKind::Cape, &test_png(64, 32, ColorType::Rgba)).is_ok());
        assert!(sanitize(TextureKind::Cape, &test_png(64, 64, ColorType::Rgba)).is_err());
        assert!(sanitize(TextureKind::Cape, &test_png(22, 18, ColorType::Rgba)).is_err());
    }
}
