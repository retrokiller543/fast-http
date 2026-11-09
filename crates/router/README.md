# fast-http-router

## Role

Static request routing. A `Router` is a typed tuple of `Route`s (pattern plus endpoints), matched without allocating and without `dyn`. It implements `fast_http_core::Routes`, so any transport can drive it. No macros are required; Upwell's macros are expected to emit the same builder calls.

## Usage

```rust
let router = Router::new((
    Route::new("/users/{id}", (get(show),))?,
    Route::new("/users", (post(create),))?,
));
```

Handlers are plain `async fn`s. Arguments are extractors (`Path<T>`, `Method`, `Uri`, `HeaderMap`, `Extension<T>`, and so on); the last one may consume the body (`Body<T>` from `fast-http-format`). Return anything that implements `IntoOutcome`, including `Result<_, E>` for any `E: ErrorResponse`.

Patterns: `/users/{id}` captures one segment, `*` matches one segment without capturing, `**` matches the remaining segments, and `{**name}` captures them (read as the borrowed `Segments<'_>` or via `PathParams::segments`).

Handlers are plain `async fn`s and may borrow from the request: `Path<(&str, u64)>`, `Path<Segments<'_>>`, `Path<BTreeMap<&str, &str>>`, `&Parts`, `&HeaderMap`, `&Method`, `&Uri` and `&Extensions` all extract without allocating.

Run `cargo run -p fast-http-router --example users` for a mocked-request walkthrough.

## Internal role

At startup `Router::new` builds an index from the first literal segment to the candidate routes, kept in registration order. A request does a binary search on that index, matches the candidates' patterns into a stack `ParamBuf` of borrowed values, and hands off to `dispatch_set`. Misses become `Rejection::NotFound` through the configured `ErrorHandler`.

## Feature flags

None.
