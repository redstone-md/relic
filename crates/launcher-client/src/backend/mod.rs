//! Everything that is not drawing: talking to the server, remembering
//! settings, caching what the server hands out.

use std::sync::LazyLock;

use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

pub mod address;
pub mod api;
pub mod assets;
pub mod game;
pub mod secrets;
pub mod store;
pub mod sync;
pub mod update;

/// Network and disk work runs here. The UI has its own executor, which is
/// not a Tokio one, so anything built on Tokio is spawned onto this runtime
/// and its handle awaited from the UI side.
static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("relic-io")
        .enable_all()
        .build()
        .expect("the I/O runtime starts")
});

pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    RUNTIME.spawn(future)
}

pub fn spawn_blocking<F, R>(work: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    RUNTIME.spawn_blocking(work)
}
