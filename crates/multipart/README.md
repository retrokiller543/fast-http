# fast-http-multipart

## Role

Streaming `multipart/form-data` extraction for fast-http, shaped after `axum::extract::Multipart` and built on `multer`. A field can be read in chunks, so an upload never has to fit in memory.

## Usage

```rust
async fn upload(mut form: Multipart) -> Result<String, MultipartError> {
    let mut names = Vec::new();

    while let Some(mut field) = form.next_field().await? {
        names.push(field.name().unwrap_or("-").to_owned());

        while let Some(chunk) = field.chunk().await? {
            sink.write_all(&chunk).await?;
        }
    }

    Ok(names.join(","))
}
```

- `Multipart` is a body extractor, so it is the last handler argument.
- `Multipart<WHOLE, FIELD>` caps the whole body and each field, in bytes (both default to 8 MiB, the framework's `DEFAULT_BODY_LIMIT`). Exceeding either gives `MultipartError::TooLarge` (413).
- The server's `max_body_bytes` also applies, and is 8 MiB by default as well. Raise it through `Settings` for larger uploads; a body cut off by that limit also reports `TooLarge`.
- `MultipartError` renders itself: 415 when the request is not multipart, 400 for a missing boundary or a malformed body, 413 for size limits.

## Internal role

`from_request` turns the request body into a stream of `Bytes` (zero-copy for `Bytes` bodies), parses the boundary from `Content-Type` and hands both to `multer` with the size constraints. `multer` boxes the stream once per upload; nothing else allocates per request. Its error type is mapped to `MultipartError` and not exposed.

## Feature flags

None.
