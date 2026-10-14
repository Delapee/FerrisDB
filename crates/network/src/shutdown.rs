use tokio::io;
use tokio_util::sync::CancellationToken;

pub fn spawn_shutdown_listener(token: CancellationToken) {
    tokio::spawn(async move {
        match wait_for_shutdown_signal().await {
            Ok(sig_name) => {
                tracing::info!(
                    signal = sig_name,
                    "Shutdown signal received; initiating graceful stop"
                );
            }
            Err(err) => {
                tracing::error!(error = %err, "Failed to listen for OS signals; forcing shutdown");
            }
        }

        token.cancel();
    });
}

async fn wait_for_shutdown_signal() -> io::Result<&'static str> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sigquit = signal(SignalKind::quit())?;

        tokio::select! {
            res = tokio::signal::ctrl_c() => {
                res?;
                Ok("SIGINT (Ctrl+C)")
            }
            _ = sigint.recv()  => Ok("SIGINT"),
            _ = sigterm.recv() => Ok("SIGTERM"),
            _ = sigquit.recv() => Ok("SIGQUIT"),
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await?;
        Ok("SIGINT (Ctrl+C)")
    }
}
