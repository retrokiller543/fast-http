# fast-http-multipart

## Role

Streaming `multipart/form-data` extraction for fast-http, shaped after `axum::extract::Multipart` and built on `multer`. A field can be read in chunks, so an upload never has to fit in memory.

## Usage

```rust
use std::sync::atomic::{AtomicU64, Ordering};

use fast_http_core::HandlerError;
use fast_http_multipart::Multipart;
use tokio::{fs::OpenOptions, io::AsyncWriteExt};

static NEXT_UPLOAD: AtomicU64 = AtomicU64::new(0);

async fn upload(mut form: Multipart) -> Result<String, HandlerError> {
    let mut names = Vec::new();

    while let Some(mut field) = form.next_field().await? {
        let id = NEXT_UPLOAD.fetch_add(1, Ordering::Relaxed);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(format!("uploads/{id}"))
            .await
            .map_err(HandlerError::internal)?;

        while let Some(chunk) = field.chunk().await? {
            file.write_all(&chunk).await.map_err(HandlerError::internal)?;
        }

        file.flush().await.map_err(HandlerError::internal)?;
        names.push(field.name().unwrap_or("-").to_owned());
    }

    Ok(names.join(","))
}
```

The `uploads` directory must exist; create it at startup. Each field gets a fresh id and `create_new` refuses to open an existing file, so no upload overwrites another, across requests or after a restart (where it fails with a 500 rather than clobber). The file is flushed before the field counts as stored, so a failed final write is an error, not a silent truncation. A `MultipartError` converts into `HandlerError` with its own status through `?`; an I/O error becomes an opaque `500` through `HandlerError::internal`. Each `field` is dropped at the end of its loop iteration, which is what lets `next_field` move on: a field must be dropped before the next one is read, even after `chunk()` returned `None`.

- `Multipart` is a body extractor, so it is the last handler argument.
- `Multipart<WHOLE, FIELD>` caps the whole body and each field, in bytes (both default to 8 MiB, the framework's `DEFAULT_BODY_LIMIT`). Exceeding either gives `MultipartError::TooLarge` (413).
- The server's `max_body_bytes` also applies, and is 8 MiB by default as well. Raise it through `Settings` for larger uploads; a body cut off by that limit also reports `TooLarge`.
- `MultipartError` renders itself: 415 when the request is not multipart, 400 for a missing boundary or a malformed body, 413 for size limits.

## Internal role

`from_request` turns the request body into a stream of `Bytes` (zero-copy for `Bytes` bodies), parses the boundary from `Content-Type` and hands both to `multer` with the size constraints. `multer` boxes the stream once per upload; nothing else allocates per request. Its error type is mapped to `MultipartError` and not exposed.

## Feature flags

None.
