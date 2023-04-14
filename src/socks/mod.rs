use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{debug, error, info, warn};

mod v4;
mod v5;

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
    let method_count = client_conn.read_u8().await?;

    let mut methods: Vec<u8> = vec![0; method_count as usize];
    client_conn.read_exact(&mut methods).await?;
    if !methods.contains(&v5::AUTH_NOT_REQUIRED) {
        let resp: [u8; 2] = v5::AuthResponse::none().to_buf();
        client_conn.write_all(&resp).await?;
        return Ok(());
    }
    client_conn
        .write_all(&v5::AuthResponse::not_required().to_buf())
        .await?;

    let _ver = client_conn.read_u8().await?;
    let cmd = client_conn.read_u8().await?;
    let _rsv = client_conn.read_u8().await?;
    let atype = client_conn.read_u8().await?;

    match cmd {
        v5::CMD_CONNECT => {}
        v5::CMD_BIND => {
            error!(?cmd, "unsupported command");
            client_conn
                .write_all(&v5::Response::unsupported_command().to_buf())
                .await?;
            return Ok(());
        }
        v5::CMD_UDP_ASSOCIATE => {
            error!(?cmd, "unsupported command");
            client_conn
                .write_all(&v5::Response::unsupported_command().to_buf())
                .await?;
            return Ok(());
        }
        _ => {
            error!(?cmd, "unsupported command");
            client_conn
                .write_all(&v5::Response::unsupported_command().to_buf())
                .await?;
            return Ok(());
        }
    }

    let address = match atype {
        v5::ATYPE_IPV4 => {
            let mut addr: [u8; 4] = [0; 4];
            client_conn.read_exact(&mut addr).await?;
            v5::Address::IPv4(addr)
        }
        v5::ATYPE_IPV6 => {
            let mut addr: [u8; 16] = [0; 16];
            client_conn.read_exact(&mut addr).await?;
            v5::Address::IPv6(addr)
        }
        v5::ATYPE_DNS => {
            let size = client_conn.read_u8().await?;
            let mut buf: Vec<u8> = vec![0; size as usize];
            client_conn.read_exact(&mut buf).await?;

            v5::Address::DNS(String::from_utf8(buf)?)
        }
        _ => {
            error!(?atype, "unsupported address type");
            client_conn
                .write_all(&v5::Response::unsupported_address().to_buf())
                .await?;
            return Ok(());
        }
    };

    let port = client_conn.read_u16().await?;

    info!(?address, ?port, "v5 connection");

    client_conn
        .write_all(&v5::Response::success(address, port).to_buf())
        .await?;

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
