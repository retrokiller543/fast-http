//! axum serving the comparison API.

use fast_http_bench::apps;
use fast_http_load::serve::{self, BIND, Options, ServeError, alloc::Counting, control};
use tokio::net::TcpListener;

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn main() -> Result<(), ServeError> {
    let options = Options::from_args()?;
    let control = control::spawn()?;

    serve::runtime(options.workers, options.cpus)?.block_on(async {
        let listener = TcpListener::bind(BIND).await?;

        serve::announce(listener.local_addr()?, control)?;
        axum::serve(listener, apps::axum()).await?;

        Ok(())
    })
}
