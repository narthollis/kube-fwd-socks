pub(crate) mod socks;

use futures::{StreamExt as FuturesStreamExt, TryStreamExt};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use tokio::net::TcpListener;
use tokio_stream::{wrappers::TcpListenerStream, StreamExt};

use kube::Client;

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
        .with_max_level(tracing::Level::INFO)
        .init();

    let client = Client::try_default().await?;

    let socket_v4 = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 1080))).await?;
    let socket_v6 = TcpListener::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, 1080))).await?;

    info!(address = ?[socket_v4.local_addr()?, socket_v6.local_addr()? ], "Bound, Ctrl+C to stop");

    TcpListenerStream::new(socket_v4)
        .merge(TcpListenerStream::new(socket_v6))
        .take_until(tokio::signal::ctrl_c())
        .try_for_each(|client_conn| async {
            let _connection_span = info_span!(
                "connection",
                peer_addr = client_conn.peer_addr()?.to_string()
            )
            .entered();
            trace!("accepted new connection");

            let c = client.clone();

            tokio::spawn(
                async move {
                    if let Err(e) = socks::handle(client_conn, c).await {
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
