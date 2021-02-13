use iced::Executor;
use futures::Future;

/// Implementation of Tokio executor for iced
/// This does two notable things:
/// 1. Lets us use a modern Tokio, bypassing iced's dependency on tokio 0.2.x
/// 2. Lets us use our global runtime instead of spawning a second one just for the GUI.
pub struct TokioExecutor {
    rt: tokio::runtime::Handle
}

impl Executor for TokioExecutor {
    fn new() -> Result<Self, futures::io::Error> {
        Ok(TokioExecutor { rt: crate::TOKIO_RUNTIME.handle().clone() })
    }

    fn spawn(&self, future: impl Future<Output=()> + Send + 'static) {
        let _join_handle = self.rt.spawn(future);
    }

    fn enter<R>(&self, f: impl FnOnce() -> R) -> R {
        let _enter_guard = self.rt.enter();
        f()
    }
}
