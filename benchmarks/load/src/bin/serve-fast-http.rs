//! fast-http serving the comparison API.

use fast_http_bench::apps;
use fast_http_load::serve::{self, BIND, Options, ServeError, alloc::Counting, control};
use fast_http_server::Server;
use fast_http_transport::Tcp;

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn main() -> Result<(), ServeError> {
    let options = Options::from_args()?;
    let control = control::spawn()?;

    serve::runtime(options.workers, options.cpus)?.block_on(async {
        let server = Server::bind(&Tcp::new(), BIND).await?;

        serve::announce(server.local_addr()?, control)?;
        server
            .serve(apps::fast_http(), std::future::pending())
            .await;

        Ok(())
    })
}
