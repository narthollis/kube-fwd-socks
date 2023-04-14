use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{info, debug, error, warn};

mod v4;
mod v5;

pub(crate) async fn handle(mut client_conn: impl AsyncRead + AsyncWrite + Unpin) -> anyhow::Result<()> {
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
    let _method = client_conn.read_u8().await?;
    let mut dest_addr: [u8; 4] = [0, 0, 0, 0];
    let dest_port = client_conn.read_u16().await?;
    client_conn.read_exact(&mut dest_addr).await?;
    
    // Read unused userid block
    discard_until_null(&mut client_conn).await?;
    
    if dest_addr == v4::SOCKS4A_ADDRESS {
        let addr = read_until_null(&mut client_conn).await?;
        info!(port = dest_port, addr, "client requested 4a - we should be able to handle this");
        
        client_conn.write_all((&v4::Response::granted(dest_port, dest_addr)).into()).await?;
    } else {
        warn!(?dest_port, ?dest_addr, "client requested version 4, rejecting");
        
        client_conn.write_all((&v4::Response::rejected_or_failed(dest_port, dest_addr)).into()).await?;
    }

    client_conn.flush().await?;    
    Ok(())
}

async fn handle_v5(mut client_conn: impl AsyncRead + AsyncWrite + Unpin) -> anyhow::Result<()> {
    let method_count = client_conn.read_u8().await?;
    
    let mut methods: Vec<u8> = vec![0; method_count as usize];
    client_conn.read_exact(&mut methods).await?;
    if !methods.contains(&v5::AUTH_NOT_REQUIRED) {
        let resp: [u8; 2] = v5::AuthResponse::none().into();
        client_conn.write_all(&resp).await?;
        return Ok(());
    }
    self::
    
    info!("v5 connection with {} methods ({:?})", method_count, methods);
    
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
    UnsupportedVersion(u8)
}
