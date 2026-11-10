//! actix-web serving the comparison API, with its usual single-threaded runtime per worker.

use std::net::TcpListener;

use actix_web::{App, HttpServer, rt::System};
use fast_http_bench::apps;
use fast_http_load::serve::{self, BIND, Options, ServeError, alloc::Counting, control, pin};

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn main() -> Result<(), ServeError> {
    let options = Options::from_args()?;
    let control = control::spawn()?;
    let listener = TcpListener::bind(BIND)?;
    let addr = listener.local_addr()?;

    System::new().block_on(async move {
        let cpus = options.cpus;
        let server = HttpServer::new(move || {
            if let Some(cpus) = &cpus {
                pin::current_thread(cpus);
            }

            App::new().configure(apps::actix)
        })
        .workers(options.workers)
        .disable_signals()
        .listen(listener)?
        .run();

        serve::announce(addr, control)?;
        server.await?;

        Ok(())
    })
}
