# fast-http-layer

## Role

Middleware for fast-http. A middleware is a `Flow` (defined in `fast-http-core`): it receives the request, a `Next` continuation and the scope's `ErrorHandler`, and returns a `Response`. Flows are static generics, so a stack of them compiles to nested futures with no `dyn` and no boxing. This crate provides the stock flows and an optional tower adapter.

## Usage

```rust
let router = Router::new()
    .route(Route::new("/admin/{id}")?.get(show).layer(RequireAdmin))
    .route(Route::new("/health")?.get(health))
    .layer(map_request(|parts| { parts.headers.remove("x-internal"); }))
    .layer(TowerFlow::new(tower_http::trace::TraceLayer::new_for_http()));
```

- `Router::layer` runs before matching. `Route::layer` runs after matching, and the route matches the request again, so a flow that rewrites the path to something the route no longer matches gets a 404.
- The layer added last runs outermost, as in axum.
- Flows reject with `reject(errors, req, err)`, so errors render through the scope's central `ErrorHandler`.
- Stock flows:
  - `Branch`: chooses between two flows, using a predicate on `Parts` or a `HeadGuard`.
  - `Scope`: runs a flow only for paths under a prefix (matched on a segment boundary).
  - `Fallback`: tries a primary flow and, when it rejects with a 4xx without continuing (the check is configurable), runs a secondary flow instead. The primary sees only a copy of the head.
  - `Tee`: runs a flow and mirrors a copy of the request head through a shadow flow whose result is discarded.
  - `MapRequest`/`MapResponse`: synchronous edits of the request head or the response.
  - `TowerFlow` (requires the `tower` feature): wraps any `tower::Layer`. Layer errors go through `map_err` and then the error handler.

## Internal role

`Fallback` and `Tee` hand the flows they run on the side an empty body and empty extensions (`Fallback` restores the real ones when its primary continues). `Tee` polls the shadow next to the primary and drops it when the primary finishes. A router or route with no layers has the type parameter `Identity` (`PASSTHROUGH = true`). It keeps the borrowed, zero-allocation dispatch path. A layered route rebuilds the owned request after matching. `TowerFlow` boxes the body, the continuation and its future on each request, and clones the head for error rendering, so use it only where a native flow isn't worth writing. Layers that clone or retry the inner service (`Buffer`, `Retry`) aren't supported, because the continuation is single-use.

## Feature flags

- `tower`: enables `TowerFlow`, `NextService`, `TowerBody` and `BodyError`, through `tower-layer` and `tower-service`.
