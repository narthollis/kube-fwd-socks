use kube::Client;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{debug, error, info, warn};

use crate::socks::resolver::PodResolver;

mod resolver;
mod v4;
mod v5;

pub(crate) async fn handle(
    client_conn: tokio::net::TcpStream,
    kube_client: Client,
) -> anyhow::Result<()> {
    let mut buf = [0x0_u8; 1];
    client_conn.peek(&mut buf).await?;

    let ver = buf[0];

    debug!("handling connection with version {}", ver);

    let mut resolver = PodResolver::new(kube_client);

    let res = match ver {
        v4::VERSION => handle_v4(client_conn).await,
        v5::VERSION => handle_v5(client_conn, &mut resolver).await,
        _ => Err(Errors::UnsupportedVersion(ver).into()),
    };

    resolver.join().await?;
    res?;

    Ok(())
}

async fn handle_v4(mut client_conn: impl AsyncRead + AsyncWrite + Unpin) -> anyhow::Result<()> {
    let _ver = client_conn.read_u8().await?;

    let method = client_conn.read_u8().await?;
    let mut dest_addr: [u8; 4] = [0; 4];
    let dest_port = client_conn.read_u16().await?;
    client_conn.read_exact(&mut dest_addr).await?;

    if method == v4::METHOD_BIND {
        warn!("client requested bind, rejecting");
        client_conn
            .write_all(&v4::Response::rejected_or_failed(dest_port, dest_addr).to_buf())
            .await?;

        return Ok(());
    }
    if method != v4::METHOD_CONNECT {
        warn!("client requested unknown method, rejecting");
        client_conn
            .write_all(&v4::Response::rejected_or_failed(dest_port, dest_addr).to_buf())
            .await?;

        return Ok(());
    }

    // Read unused userid block
    discard_until_null(&mut client_conn).await?;

    if dest_addr == v4::SOCKS4A_ADDRESS {
        let addr = read_until_null(&mut client_conn).await?;
        info!(
            port = dest_port,
            addr, "client requested 4a - we should be able to handle this"
        );

        client_conn
            .write_all(&v4::Response::granted(dest_port, dest_addr).to_buf())
            .await?;
    } else {
        warn!(
            ?dest_port,
            ?dest_addr,
            "client requested version 4, rejecting"
        );

        client_conn
            .write_all(&v4::Response::rejected_or_failed(dest_port, dest_addr).to_buf())
            .await?;
    }

    client_conn.flush().await?;
    Ok(())
}

async fn handle_v5(
    mut client: impl AsyncRead + AsyncWrite + Unpin,
    resolver: &mut PodResolver,
) -> anyhow::Result<()> {
    let auth_request = client.receive::<v5::AuthRequest>().await?;

    if !auth_request.contains(&v5::AuthMethods::NotRequired) {
        client.send(v5::AuthResponse::none()).await?;
        return Ok(());
    }

    client.send(v5::AuthResponse::not_required()).await?;

    let req = match client.receive::<v5::CommandRequest>().await {
        Ok(c) => Ok(c),
        Err(v5::ParseError::ProtocolError(e)) => {
            error!(error = ?e, "command parse failed");
            let resp: v5::ConnectResponse = e.into();
            client.send(resp).await?;
            return Ok(());
        }
        Err(e) => Err(e),
    }?;

    info!(request = ?req, "valid v5 command");

    if req.command != v5::Command::Connect {
        warn!(?req.command, "unsupported command");
        client
            .send(v5::ConnectResponse::unsupported_command())
            .await?;
        return Ok(());
    }

    let address = match req.address {
        v5::Address::IpAddr(_) => {
            warn!(?req.address, "unsupported address");
            client
                .send(v5::ConnectResponse::unsupported_command())
                .await?;
            return Ok(());
        }
        v5::Address::Dns(ref a) => a.clone(),
    };

    let mut pod_stream = match resolver.forwarder(address.as_str(), req.port).await {
        Ok(s) => s,
        Err(e) => {
            warn!(error = ?e, "failed to resolve and open forward stream");
            client
                .send(match e {
                    resolver::Errors::PodNotFound {
                        namespace: _,
                        pod: _,
                    } => v5::ConnectResponse::host_unreachable(req.address, req.port),
                    resolver::Errors::ServiceNotFound {
                        namespace: _,
                        service: _,
                    } => v5::ConnectResponse::host_unreachable(req.address, req.port),
                    resolver::Errors::NamedServicePodsNotFound {
                        namespace: _,
                        service: _,
                        pod: _,
                    } => v5::ConnectResponse::host_unreachable(req.address, req.port),
                    resolver::Errors::PortNotFound(_, _, _) => {
                        v5::ConnectResponse::connection_refused(req.address, req.port)
                    }
                    resolver::Errors::UnsupportedAddress(_) => {
                        v5::ConnectResponse::unsupported_address()
                    }
                    resolver::Errors::ForwardFailed(_) => v5::ConnectResponse::geneal_failure(),
                    resolver::Errors::LookupFailed(_) => v5::ConnectResponse::geneal_failure(),
                    resolver::Errors::ServiceInvalid {
                        namespace: _,
                        service: _,
                        reason: _,
                    } => v5::ConnectResponse::geneal_failure(),
                    resolver::Errors::ServiceNoReadyPods {
                        namespace: _,
                        service: _,
                    } => v5::ConnectResponse::connection_refused(req.address, req.port),
                })
                .await?;
            return Ok(());
        }
    };

    client
        .send(v5::ConnectResponse::success(req.address, req.port))
        .await?;

    tokio::io::copy_bidirectional(&mut client, &mut pod_stream).await?;
    drop(pod_stream);

    Ok(())
}

async fn discard_until_null(stream: &mut (impl AsyncRead + Unpin)) -> anyhow::Result<()> {
    while stream.read_u8().await? != 0 {}
    Ok(())
}

async fn read_until_null(stream: &mut (impl AsyncRead + Unpin)) -> anyhow::Result<String> {
    let mut resp: String = String::new();

    let mut next: u8 = stream.read_u8().await?;
    while next != 0 {
        resp.push(next.into());
        next = stream.read_u8().await?;
    }

    Ok(resp)
}

pub(crate) trait Request {
    type Error;
    async fn parse(stream: &mut (impl AsyncReadExt + Unpin)) -> Result<Self, Self::Error>
    where
        Self: std::marker::Sized;
}

trait LocalAsyncReadWriteExt {
    async fn receive<M: Request>(&mut self) -> Result<M, M::Error>;
    async fn send<'a, I: Into<Vec<u8>>>(&mut self, v: I) -> std::io::Result<()>;
}
impl<T: AsyncRead + AsyncWrite + Unpin> LocalAsyncReadWriteExt for T {
    async fn send<'a, I: Into<Vec<u8>>>(&mut self, v: I) -> std::io::Result<()> {
        self.write_all(&v.into()).await
    }

    async fn receive<M: Request>(&mut self) -> Result<M, M::Error> {
        M::parse(self).await
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Errors {
    #[error("Unsupported version {0} requested")]
    UnsupportedVersion(u8),
}
