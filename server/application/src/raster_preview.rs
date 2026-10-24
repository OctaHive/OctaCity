//! Shared bounded raster decoding for explicit previews and uploaded presentation metadata.
use crate::ApplicationError;

pub(crate) fn png(media_type: &str, bytes: Vec<u8>) -> Result<Vec<u8>, ApplicationError> {
  let format = match media_type {
    "image/png" => image::ImageFormat::Png,
    "image/jpeg" => image::ImageFormat::Jpeg,
    _ => image::ImageFormat::WebP,
  };
  if image::guess_format(&bytes).ok() != Some(format) {
    return Err(ApplicationError::unavailable());
  }
  let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
  let mut limits = image::Limits::default();
  limits.max_image_width = Some(MAX_RASTER_DIMENSION);
  limits.max_image_height = Some(MAX_RASTER_DIMENSION);
  limits.max_alloc = Some(MAX_RASTER_ALLOC_BYTES);
  reader.limits(limits);
  let decoded = reader.decode().map_err(|_| ApplicationError::unavailable())?;
  let mut output = std::io::Cursor::new(Vec::new());
  decoded
    .write_to(&mut output, image::ImageFormat::Png)
    .map_err(|_| ApplicationError::unavailable())?;
  let bytes = output.into_inner();
  if bytes.len() as u64 > MAX_RASTER_BYTES {
    return Err(ApplicationError::unavailable());
  }
  Ok(bytes)
}
pub(crate) const MAX_RASTER_DIMENSION: u32 = 4096;
pub(crate) const MAX_RASTER_ALLOC_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_RASTER_BYTES: u64 = 64 * 1024;
