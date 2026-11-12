//! Branding the server supplies (logo, background, font) and player skins:
//! fetched once, kept on disk, revalidated cheaply.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::imageops::{self, FilterType};
use image::{ImageFormat, RgbaImage};
use reqwest::StatusCode;
use reqwest::header::{ETAG, IF_NONE_MATCH};
use sha2::{Digest, Sha256};

/// Nothing a theme needs is bigger than this; anything larger is a mistake
/// or an attempt to fill the disk.
const MAX_ASSET: usize = 16 * 1024 * 1024;

fn cache_path(dir: &Path, url: &str) -> PathBuf {
    dir.join(hex::encode(Sha256::digest(url.as_bytes())))
}

/// Fetches `url`, going through the on-disk cache in `dir`.
///
/// A cached copy is revalidated with its ETag, and used as is when the
/// server cannot be reached, so the launcher looks the same offline.
pub async fn fetch(http: &reqwest::Client, dir: &Path, url: &str) -> Option<Vec<u8>> {
    let path = cache_path(dir, url);
    let etag_path = path.with_extension("etag");
    let cached = tokio::fs::read(&path).await.ok();

    let mut request = http.get(url);
    if cached.is_some()
        && let Ok(etag) = tokio::fs::read_to_string(&etag_path).await
    {
        request = request.header(IF_NONE_MATCH, etag);
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(_) => return cached,
    };
    if response.status() == StatusCode::NOT_MODIFIED {
        return cached;
    }
    if !response.status().is_success() {
        return cached;
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_ASSET as u64)
    {
        return cached;
    }
    let etag = response
        .headers()
        .get(ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let bytes = match response.bytes().await {
        Ok(bytes) if bytes.len() <= MAX_ASSET => bytes.to_vec(),
        _ => return cached,
    };

    if tokio::fs::create_dir_all(dir).await.is_ok() && tokio::fs::write(&path, &bytes).await.is_ok()
    {
        match etag {
            Some(etag) => drop(tokio::fs::write(&etag_path, etag).await),
            None => drop(tokio::fs::remove_file(&etag_path).await),
        }
    }
    Some(bytes)
}

fn png(image: &RgbaImage) -> Option<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// The face of a skin, hat layer included, as a crisp `size`×`size` PNG.
pub fn skin_head(skin_png: &[u8], size: u32) -> Option<Vec<u8>> {
    let skin = image::load_from_memory_with_format(skin_png, ImageFormat::Png)
        .ok()?
        .to_rgba8();
    if skin.width() != 64 || !matches!(skin.height(), 32 | 64) {
        return None;
    }
    let mut face = imageops::crop_imm(&skin, 8, 8, 8, 8).to_image();
    let hat = imageops::crop_imm(&skin, 40, 8, 8, 8).to_image();
    imageops::overlay(&mut face, &hat, 0, 0);
    png(&imageops::resize(&face, size, size, FilterType::Nearest))
}

/// The family name of a TrueType/OpenType font, which is what text is
/// styled with once the font is loaded.
pub fn font_family(font: &[u8]) -> Option<String> {
    let face = ttf_parser::Face::parse(font, 0).ok()?;
    let names = face.names();
    let pick = |id| {
        (0..names.len())
            .filter_map(|i| names.get(i))
            .filter(|name| name.name_id == id && name.is_unicode())
            .find_map(|name| name.to_string())
    };
    pick(ttf_parser::name_id::TYPOGRAPHIC_FAMILY).or_else(|| pick(ttf_parser::name_id::FAMILY))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn head_is_the_face_with_the_hat_on_top() {
        let mut skin = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 0]));
        for y in 8..16 {
            for x in 8..16 {
                skin.put_pixel(x, y, Rgba([200, 150, 100, 255]));
            }
        }
        // One opaque hat pixel over the top-left of the face.
        skin.put_pixel(40, 8, Rgba([10, 20, 30, 255]));
        let head = skin_head(&png(&skin).unwrap(), 64).unwrap();
        let head = image::load_from_memory(&head).unwrap().to_rgba8();
        assert_eq!(head.dimensions(), (64, 64));
        assert_eq!(head.get_pixel(0, 0), &Rgba([10, 20, 30, 255]));
        assert_eq!(head.get_pixel(63, 63), &Rgba([200, 150, 100, 255]));
    }

    #[test]
    fn odd_sized_images_are_not_skins() {
        let image = RgbaImage::from_pixel(32, 32, Rgba([1, 2, 3, 255]));
        assert!(skin_head(&png(&image).unwrap(), 64).is_none());
        assert!(skin_head(b"not a png", 64).is_none());
    }

    #[test]
    fn cache_file_is_named_after_the_url() {
        let a = cache_path(Path::new("c"), "https://example.com/logo.png");
        let b = cache_path(Path::new("c"), "https://example.com/logo2.png");
        assert_ne!(a, b);
        assert_eq!(a.file_name().unwrap().len(), 64);
    }
}
