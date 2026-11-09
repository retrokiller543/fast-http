# fast-http-bench

## Role

The same small JSON API (health, user lookup, user creation, a two-parameter repo route and a catch-all file route) written on fast-http, axum and actix-web, plus the Criterion benches that compare them in process. Never published.

## Usage

```sh
just bench routing          # router lookup: fast-http vs matchit vs actix-router
just bench stack -- --quick # full request dispatch without a network
```

`fast_http_bench::apps::{fast_http, axum, actix}` build the apps; `fast-http-load` serves them over TCP.

## Internal role

`apps.rs` keeps the three implementations side by side so a route added to one is added to all. Handlers share the `User`/`ApiError` types; `ApiError` implements each framework's error-response trait. The benches use `fast-http-bench-support` for request fixtures.

## Feature flags

None.
