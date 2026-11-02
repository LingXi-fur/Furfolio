use crate::error::AppError;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use std::io::Cursor;

pub struct DecodedMedia {
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
    pub preview: Vec<u8>,
}
pub fn validate_dimensions(width: u32, height: u32, allocation: u64) -> Result<(), AppError> {
    if width == 0 || height == 0 {
        return Err(AppError::InvalidMedia);
    }
    if width > 12000
        || height > 12000
        || u64::from(width) * u64::from(height) > 24_000_000
        || allocation > 128 * 1024 * 1024
    {
        return Err(AppError::MediaTooLarge);
    }
    Ok(())
}
pub fn decode_media(bytes: &[u8]) -> Result<DecodedMedia, AppError> {
    if bytes.len() as u64 > super::media_store::MAX_SOURCE {
        return Err(AppError::MediaTooLarge);
    }
    let format = image::guess_format(bytes).map_err(|_| AppError::InvalidMedia)?;
    let mime = match format {
        ImageFormat::Png => {
            reject_animation_chunks(bytes, true)?;
            "image/png"
        }
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::WebP => {
            reject_animation_chunks(bytes, false)?;
            "image/webp"
        }
        _ => return Err(AppError::InvalidMedia),
    };
    let mut limits = Limits::default();
    limits.max_image_width = Some(12000);
    limits.max_image_height = Some(12000);
    limits.max_alloc = Some(128 * 1024 * 1024);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits.clone());
    let mut decoder = reader.into_decoder().map_err(decode_error)?;
    let (width, height) = decoder.dimensions();
    validate_dimensions(width, height, decoder.total_bytes())?;
    decoder.set_limits(limits).map_err(decode_error)?;
    let image = DynamicImage::from_decoder(decoder).map_err(decode_error)?;
    let preview = image.thumbnail(1280, 1280).to_rgba8();
    drop(image);
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(preview)
        .write_to(&mut output, ImageFormat::Png)
        .map_err(decode_error)?;
    let preview = output.into_inner();
    if preview.len() > 4 * 1024 * 1024 {
        return Err(AppError::MediaTooLarge);
    }
    Ok(DecodedMedia {
        mime,
        width,
        height,
        preview,
    })
}
fn decode_error(error: image::ImageError) -> AppError {
    match error {
        image::ImageError::Limits(_) => AppError::MediaTooLarge,
        _ => AppError::InvalidMedia,
    }
}
fn reject_animation_chunks(bytes: &[u8], png: bool) -> Result<(), AppError> {
    let mut offset = if png { 8usize } else { 12usize };
    if !png {
        let declared = u32::from_le_bytes(
            bytes
                .get(4..8)
                .ok_or(AppError::InvalidMedia)?
                .try_into()
                .unwrap(),
        ) as usize;
        if declared.checked_add(8) != Some(bytes.len()) {
            return Err(AppError::InvalidMedia);
        }
    }
    let mut ended = false;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset + 8)
            .ok_or(AppError::InvalidMedia)?;
        let (kind, length) = if png {
            (
                &header[4..8],
                u32::from_be_bytes(header[..4].try_into().unwrap()) as usize,
            )
        } else {
            (
                &header[..4],
                u32::from_le_bytes(header[4..].try_into().unwrap()) as usize,
            )
        };
        if (png && matches!(kind, b"acTL" | b"fcTL" | b"fdAT"))
            || (!png && matches!(kind, b"ANIM" | b"ANMF"))
        {
            return Err(AppError::InvalidMedia);
        }
        let end = offset
            .checked_add(8)
            .and_then(|n| n.checked_add(length))
            .and_then(|n| n.checked_add(if png { 4 } else { length % 2 }))
            .ok_or(AppError::InvalidMedia)?;
        if end > bytes.len() {
            return Err(AppError::InvalidMedia);
        }
        if !png && kind == b"VP8X" && bytes.get(offset + 8).is_some_and(|flags| flags & 2 != 0) {
            return Err(AppError::InvalidMedia);
        }
        offset = end;
        if png && kind == b"IEND" {
            ended = true;
            break;
        }
    }
    if offset != bytes.len() || (png && !ended) {
        return Err(AppError::InvalidMedia);
    }
    Ok(())
}
