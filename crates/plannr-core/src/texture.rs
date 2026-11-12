//! Texture images: a material carries its image in the document, as a `data:` URL holding a PNG or a JPEG.

/// The longest image a material takes, as text (about 1 MB of image).
pub const MAX_IMAGE_CHARS: usize = 1_400_000;
/// All the images of one document together.
pub const MAX_DOCUMENT_CHARS: usize = 12_000_000;

/// The media type of an image this engine takes, or `None`.
pub fn mime(url: &str) -> Option<&'static str> {
    if url.starts_with("data:image/png;base64,") { Some("image/png") } else if url.starts_with("data:image/jpeg;base64,") { Some("image/jpeg") } else { None }
}

/// The image file inside a `data:` URL.
pub fn bytes(url: &str) -> Option<Vec<u8>> {
    mime(url)?;
    let text = url.split_once(',')?.1.trim_end_matches('=');
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let v = match c { b'A'..=b'Z' => c - b'A', b'a'..=b'z' => c - b'a' + 26, b'0'..=b'9' => c - b'0' + 52, b'+' => 62, b'/' => 63, _ => return None };
        acc = (acc << 6) | v as u32; bits += 6;
        if bits >= 8 { bits -= 8; out.push((acc >> bits) as u8); acc &= (1 << bits) - 1; }
    }
    Some(out)
}

/// Width and height in pixels, read from the head of a PNG or JPEG file.
pub fn pixels(file: &[u8]) -> Option<(u32, u32)> {
    let be = |i: usize, n: usize| file.get(i..i + n).map(|b| b.iter().fold(0u32, |a, x| (a << 8) | *x as u32));
    if file.starts_with(b"\x89PNG\r\n\x1a\n") { return Some((be(16, 4)?, be(20, 4)?)).filter(|(w, h)| *w > 0 && *h > 0); }
    if !file.starts_with(&[0xFF, 0xD8]) { return None; }
    let mut i = 2;
    loop {
        if *file.get(i)? != 0xFF { return None; }
        let marker = *file.get(i + 1)?;
        if marker == 0xFF { i += 1; continue; } // fill bytes
        if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
            return Some((be(i + 7, 2)?, be(i + 5, 2)?)).filter(|(w, h)| *w > 0 && *h > 0);
        }
        i += 2 + be(i + 2, 2)? as usize;
    }
}

/// An image a material may carry: its size in pixels, or what is wrong with it.
pub fn check(url: &str) -> Result<(u32, u32), String> {
    if mime(url).is_none() { return Err("a texture image is a data: URL holding a PNG or JPEG (data:image/png;base64,… or data:image/jpeg;base64,…)".into()); }
    if url.len() > MAX_IMAGE_CHARS { return Err(format!("this texture image is too large: {} KB, at most {} KB (make it smaller, 1024 pixels is plenty)", url.len() * 3 / 4 / 1024, MAX_IMAGE_CHARS * 3 / 4 / 1024)); }
    bytes(url).as_deref().and_then(pixels).ok_or_else(|| "this texture image cannot be read as a PNG or JPEG".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    // a 2 × 1 pixel PNG
    const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAIAAAB7QOjdAAAAD0lEQVR4nGPYkGRfYaMBAAkVAi5VLbR0AAAAAElFTkSuQmCC";
    #[test]
    fn a_png_gives_its_size_and_other_things_are_refused() {
        assert_eq!(check(PNG), Ok((2, 1)));
        assert!(check("data:image/svg+xml;base64,AAAA").is_err());
        assert!(check("data:image/png;base64,!!!!").is_err());
        assert!(check("data:image/png;base64,AAAA").is_err());
        assert!(check("https://example.com/a.png").is_err());
    }
    #[test]
    fn a_jpeg_head_gives_its_size() {
        // SOI, an APP0 segment of 4 bytes, then SOF0 with height 3 and width 5
        let jpg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x03, 0x00, 0x05, 0x01];
        assert_eq!(pixels(&jpg), Some((5, 3)));
        assert_eq!(pixels(&jpg[..9]), None);
    }
}
