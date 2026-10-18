//! The reader's own pictures: staged from the composer as draft rows,
//! restaged for "show to the model again", and prepared for a model that can
//! see, once per size, into the `prepared/` cache (chat-vision §7, §10).

use base64::prelude::{Engine as _, BASE64_STANDARD};
use lattice_core::application::contracts::images::{ImageOrigin, PreparedImageDto};
use lattice_core::application::ports::chat_images::ImageTurnScope;
use lattice_core::domain::models::vision::VisionGeometry;
use lattice_core::shared::error::{AppError, Result};

use super::ChatImageService;
use crate::features::web::images::fetch::{validate_image, ImageRejected};
use crate::features::web::images::prepare::{prepare, MAX_DECODE_SIDE};
use crate::features::web::images::repository::{ImageRow, NewImageRow};
use crate::features::web::images::store::{new_id, Verified};

/// What the composer is told when a picture cannot be staged.
fn refused(file_name: &str, reason: &ImageRejected) -> AppError {
    match reason {
        ImageRejected::TooLarge | ImageRejected::TooManyPixels { .. } => AppError::InvalidInput(
            format!("{file_name}: this picture is too large to prepare ({reason})"),
        ),
        _ => AppError::InvalidInput(format!("{file_name}: {reason}")),
    }
}

impl ChatImageService {
    /// Validates `bytes` by magic bytes and header, then writes the row
    /// (leased to the draft) and the file, in that order, as a found
    /// picture is written.
    pub(super) async fn store_attached(
        &self,
        scope: &ImageTurnScope,
        file_name: &str,
        bytes: Vec<u8>,
    ) -> Result<ImageRow> {
        let image = validate_image(bytes).map_err(|reason| refused(file_name, &reason))?;
        // The decoder refuses anything wider or taller than this later; say
        // so now, while the reader is still choosing what to send.
        if image.width > MAX_DECODE_SIDE || image.height > MAX_DECODE_SIDE {
            return Err(refused(file_name, &ImageRejected::TooLarge));
        }
        let id = new_id();
        let name = file_name.trim().to_string();
        let row = NewImageRow {
            id: id.clone(),
            conversation_id: scope.conversation_id.clone(),
            request_id: scope.request_id.clone(),
            ext: image.ext.to_string(),
            mime: image.mime.to_string(),
            sha256: image.sha256.clone(),
            byte_size: image.bytes.len() as u64,
            width: image.width,
            height: image.height,
            caption: name.clone(),
            source_url: None,
            source_title: None,
            image_url: None,
            origin: ImageOrigin::Attachment,
            license: None,
            attribution: None,
            original_name: Some(name),
        };
        self.repository.insert(&row).await?;
        if let Err(error) = self.store.write(&id, image.ext, image.bytes).await {
            let _ = self.repository.delete_ids(std::slice::from_ref(&id)).await;
            return Err(error);
        }
        self.repository
            .get(&id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("chat image {id} vanished after insert")))
    }

    /// A new draft row with its own copy of the file. The row before its
    /// file, as the sweep expects.
    pub(super) async fn restage_row(
        &self,
        conversation_id: &str,
        image_id: &str,
    ) -> Result<ImageRow> {
        let request_id = format!(
            "{}{}",
            lattice_core::application::ports::chat_images::DRAFT_REQUEST_PREFIX,
            new_id()
        );
        let id = new_id();
        let row = self
            .repository
            .insert_draft_copy(image_id, &id, conversation_id, &request_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "picture {image_id} is not in conversation {conversation_id}"
                ))
            })?;
        if let Err(error) = self.store.copy(image_id, &id, &row.ext).await {
            let _ = self.repository.delete_ids(std::slice::from_ref(&id)).await;
            return Err(error);
        }
        Ok(row)
    }

    /// The picture fitted to `geometry`, read from the prepared cache or
    /// prepared now and cached. The cache is keyed by the size fitted from
    /// the row's stored dimensions, so a second request at the same size
    /// reads a file and decodes nothing; the size and cost returned are the
    /// prepared file's own (EXIF orientation can swap its sides).
    pub(super) async fn prepare_cached(
        &self,
        image_id: &str,
        geometry: VisionGeometry,
        max_tokens: Option<u32>,
    ) -> Result<Option<PreparedImageDto>> {
        let Some(row) = self.repository.get(image_id).await? else {
            return Ok(None);
        };
        let width = u32::try_from(row.width).unwrap_or(0);
        let height = u32::try_from(row.height).unwrap_or(0);
        let fitted = match max_tokens {
            Some(max_tokens) => geometry.fit_within(width, height, max_tokens),
            None => Some(geometry.plan(width, height)),
        }
        .ok_or_else(|| {
            AppError::InvalidInput(format!(
                "picture {image_id} cannot be shown within {} tokens",
                max_tokens.unwrap_or_default()
            ))
        })?;

        if let Some((_, bytes)) = self
            .store
            .read_prepared(&row.id, fitted.width, fitted.height)
            .await
        {
            if let Some(prepared) = cached(&bytes, geometry) {
                return Ok(Some(prepared));
            }
        }

        let source = match self
            .store
            .read_verified(&row.id, &row.ext, &row.sha256)
            .await
        {
            Verified::Intact(bytes) => bytes,
            Verified::Mismatch | Verified::Missing => {
                tracing::warn!(
                    image_id,
                    "A chat picture's file is missing or altered; not sent"
                );
                return Ok(None);
            }
        };
        let mime = row.mime.clone();
        let prepared =
            tokio::task::spawn_blocking(move || prepare(&source, &mime, geometry, max_tokens))
                .await
                .map_err(|e| AppError::Other(format!("picture preparation task failed: {e}")))?
                .map_err(|reason| AppError::InvalidInput(format!("{}: {reason}", row.caption)))?;
        let data_base64 = BASE64_STANDARD.encode(&prepared.bytes);
        let dto = PreparedImageDto {
            mime: prepared.mime.to_string(),
            data_base64,
            width: prepared.width,
            height: prepared.height,
            tokens: prepared.tokens,
        };
        let ext = prepared.ext();
        if let Err(error) = self
            .store
            .write_prepared(&row.id, fitted.width, fitted.height, ext, prepared.bytes)
            .await
        {
            // The picture is still sent; the next request prepares it again.
            tracing::warn!(%error, image_id, "Could not cache a prepared chat picture");
        }
        Ok(Some(dto))
    }
}

/// A cached prepared file as a request part, its size read from its header.
/// `None` when the file is not a picture the cache writes.
fn cached(bytes: &[u8], geometry: VisionGeometry) -> Option<PreparedImageDto> {
    let mime = match infer::get(bytes)?.mime_type() {
        "image/jpeg" => "image/jpeg",
        "image/png" => "image/png",
        _ => return None,
    };
    let size = imagesize::blob_size(bytes).ok()?;
    let width = u32::try_from(size.width).ok()?;
    let height = u32::try_from(size.height).ok()?;
    Some(PreparedImageDto {
        mime: mime.to_string(),
        data_base64: BASE64_STANDARD.encode(bytes),
        width,
        height,
        tokens: geometry.tokens(width, height),
    })
}

#[cfg(test)]
mod tests;
