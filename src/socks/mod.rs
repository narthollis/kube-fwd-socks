use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{debug, error, info, warn};

mod v4;
mod v5;

pub(crate) trait Request {
    async fn parse(stream: &mut (impl AsyncReadExt + Unpin)) -> anyhow::Result<Self>
    where
        Self: std::marker::Sized;
}

pub(crate) trait Response {
    async fn send(&self, stream: &mut (impl AsyncWrite + Unpin)) -> anyhow::Result<()>;
}

pub(crate) async fn handle(
    mut client_conn: impl AsyncRead + AsyncWrite + Unpin,
) -> anyhow::Result<()> {
    let ver = client_conn.read_u8().await?;

    debug!("handling connection with version {}", ver);

    match ver {
        v4::VERSION => handle_v4(client_conn).await,
        v5::VERSION => handle_v5(client_conn).await,
        _ => Err(Errors::UnsupportedVersion(ver).into()),
    }?;

    Ok(())
}

async fn handle_v4(mut client_conn: impl AsyncRead + AsyncWrite + Unpin) -> anyhow::Result<()> {
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

async fn handle_v5(mut client_conn: impl AsyncRead + AsyncWrite + Unpin) -> anyhow::Result<()> {
    let auth_request: v5::AuthRequest = v5::AuthRequest::parse(&mut client_conn).await?;

    if !auth_request.contains(&v5::AuthMethods::NotRequired) {
        v5::AuthResponse::none().send(&mut client_conn).await?;
        return Ok(());
    }

    v5::AuthResponse::not_required()
        .send(&mut client_conn)
        .await?;

    let req = match v5::CommandRequest::parse(&mut client_conn).await {
        Ok(c) => Ok(c),
        Err(e) => match e.downcast_ref::<v5::Errors>() {
            Some(e) => {
                error!(error = ?e, "command parse failed");
                let resp: v5::ConnectResponse = e.into();
                resp.send(&mut client_conn).await?;
                return Ok(());
            }
            None => Err(e),
        },
    }?;

    info!(request = ?req, "valid v5 command");

    match req.command {
        v5::Command::Connect => {
            v5::ConnectResponse::success(req.address, req.port)
                .send(&mut client_conn)
                .await?;
        }
        _ => {
            warn!(?req.command, "unsupported command");
            v5::ConnectResponse::unsupported_command()
                .send(&mut client_conn)
                .await?;
        }
    };

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

#[derive(thiserror::Error, Debug)]
pub enum Errors {
    #[error("Unsupported version {0} requested")]
    UnsupportedVersion(u8),
}
