#![feature(async_fn_in_trait)]

pub(crate) mod socks;

use futures::{StreamExt, TryStreamExt};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

use tracing::{error, info, info_span, trace, Instrument};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let format = tracing_subscriber::fmt::format()
        .without_time()
        .with_level(false)
        .with_target(false)
        .pretty()
        .with_source_location(false);
    tracing_subscriber::fmt()
        .event_format(format)
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let socket = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 1080))).await?;

    info!(address = ?socket.local_addr()?, "Bound, Ctrl+C to stop");

    TcpListenerStream::new(socket)
        .take_until(tokio::signal::ctrl_c())
        .try_for_each(|client_conn| async {
            let _connection_span = info_span!(
                "connection",
                peer_addr = client_conn.peer_addr()?.to_string()
            )
            .entered();
            trace!("accepted new connection");

            tokio::spawn(
                async move {
                    if let Err(e) = socks::handle(client_conn).await {
                        error!(
                            error = e.as_ref() as &dyn std::error::Error,
                            "failed to forward connection"
                        );
                    }
                }
                .in_current_span(),
            );

            Ok(())
        })
        .await?;

    Ok(())
}
