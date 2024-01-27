// https://www.rfc-editor.org/rfc/rfc1928

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::Request;

pub const VERSION: u8 = 5;

pub const AUTH_NOT_REQUIRED: u8 = 0x00;
pub const AUTH_GSSAPI: u8 = 0x01;
pub const AUTH_USER_PASS: u8 = 0x02;
pub const AUTH_NONE: u8 = 0xFF;

pub const CMD_CONNECT: u8 = 0x01;
pub const CMD_BIND: u8 = 0x02;
pub const CMD_UDP_ASSOCIATE: u8 = 0x3;

pub const ATYPE_IPV4: u8 = 0x01;
pub const ATYPE_IPV6: u8 = 0x04;
pub const ATYPE_DNS: u8 = 0x03;

pub const RESP_SUCCEEDED: u8 = 0x00;
pub const RESP_GENERAL_FAILURE: u8 = 0x01;
#[allow(dead_code)]
pub const RESP_DENIED: u8 = 0x02;
pub const RESP_NETWORK_UNREACHABLE: u8 = 0x03;
pub const RESP_HOST_UNREACHABLE: u8 = 0x04;
pub const RESP_CONNECTION_REFUSED: u8 = 0x05;
#[allow(dead_code)]
pub const RESP_TTL_EXPIRED: u8 = 0x06;
pub const RESP_COMMAND_NOT_SUPPORTED: u8 = 0x07;
pub const RESP_ADDRESS_NOT_SUPPORTED: u8 = 0x08;

#[repr(u8)]
#[derive(Debug, thiserror::Error)]
pub enum Errors {
    #[error("General Failure {0:?}")]
    General(#[source] anyhow::Error) = RESP_GENERAL_FAILURE,
    #[error("Unsupported Command {0}")]
    UnsupportedCommand(u8) = RESP_COMMAND_NOT_SUPPORTED,
    #[error("Unsupported Address Type {0}")]
    UnsupportedAddressType(u8) = RESP_ADDRESS_NOT_SUPPORTED,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error(transparent)]
    ProtocolError(#[from] Errors),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    String(#[from] std::string::FromUtf8Error),
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, int_enum::IntEnum)]
pub enum AuthMethods {
    NotRequired = AUTH_NOT_REQUIRED,
    Gssapi = AUTH_GSSAPI,
    Basic = AUTH_USER_PASS,
    None = AUTH_NONE,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, int_enum::IntEnum)]
pub enum Command {
    Connect = CMD_CONNECT,
    Bind = CMD_BIND,
    UdpAssociate = CMD_UDP_ASSOCIATE,
}

pub struct AuthRequest {
    requests: Vec<AuthMethods>,
}

impl AuthRequest {
    pub fn contains(&self, method: &AuthMethods) -> bool {
        self.requests.contains(method)
    }
}

impl Request for AuthRequest {
    type Error = anyhow::Error;
    async fn parse(stream: &mut (impl tokio::io::AsyncReadExt + Unpin)) -> anyhow::Result<Self>
    where
        Self: std::marker::Sized,
    {
        let ver = stream.read_u8().await?;
        if ver != VERSION {
            return Err(Errors::General(super::Errors::UnsupportedVersion(ver).into()).into());
        }

        let method_count = stream.read_u8().await?;
        if method_count == 0 {
            return Ok(AuthRequest { requests: vec![] });
        }

        let mut buf: Vec<u8> = vec![0; method_count as usize];
        stream.read_exact(&mut buf).await?;

        let requests = buf
            .into_iter()
            .filter_map(|v| AuthMethods::try_from(v).ok())
            .collect();

        Ok(AuthRequest { requests })
    }
}

pub struct AuthResponse {
    pub method: AuthMethods,
}

impl AuthResponse {
    pub fn not_required() -> AuthResponse {
        AuthResponse {
            method: AuthMethods::NotRequired,
        }
    }

    pub fn none() -> AuthResponse {
        AuthResponse {
            method: AuthMethods::None,
        }
    }
}

impl From<AuthResponse> for Vec<u8> {
    fn from(value: AuthResponse) -> Self {
        vec![VERSION, value.method as u8]
    }
}

#[derive(Debug)]
pub enum Address {
    IpAddr(IpAddr),
    Dns(String),
}

impl From<Address> for Vec<u8> {
    fn from(value: Address) -> Self {
        match value {
            Address::IpAddr(IpAddr::V4(a)) => [vec![ATYPE_IPV4], a.octets().into()].concat(),
            Address::IpAddr(IpAddr::V6(a)) => [vec![ATYPE_IPV6], a.octets().into()].concat(),
            Address::Dns(a) => [vec![ATYPE_DNS, a.len() as u8], Vec::from(a.as_bytes())].concat(),
        }
    }
}

impl From<Ipv4Addr> for Address {
    fn from(value: Ipv4Addr) -> Self {
        Address::IpAddr(IpAddr::V4(value))
    }
}
impl From<Ipv6Addr> for Address {
    fn from(value: Ipv6Addr) -> Self {
        Address::IpAddr(IpAddr::V6(value))
    }
}
impl From<IpAddr> for Address {
    fn from(value: IpAddr) -> Self {
        Address::IpAddr(value)
    }
}

#[derive(Debug)]
pub struct CommandRequest {
    pub command: Command,
    pub address: Address,
    pub port: u16,
}

impl Request for CommandRequest {
    type Error = ParseError;
    async fn parse(stream: &mut (impl tokio::io::AsyncReadExt + Unpin)) -> Result<Self, ParseError>
    where
        Self: std::marker::Sized,
    {
        let ver = stream.read_u8().await?;
        if ver != VERSION {
            return Err(Errors::General(super::Errors::UnsupportedVersion(ver).into()).into());
        }

        let command = Command::try_from(stream.read_u8().await?)
            .map_err(Errors::UnsupportedCommand)?;

        // This next byte is very literally a unused reserved byte, just read and discard
        let _rsv = stream.read_u8().await?;

        let atype = stream.read_u8().await?;
        let address = match atype {
            ATYPE_IPV4 => {
                let mut addr = [0; 4];
                stream.read_exact(&mut addr).await?;
                Ok(Ipv4Addr::from(addr).into())
            }
            ATYPE_IPV6 => {
                let mut addr = [0; 16];
                stream.read_exact(&mut addr).await?;
                Ok(Ipv6Addr::from(addr).into())
            }
            ATYPE_DNS => {
                let size = stream.read_u8().await?;
                let mut buf = vec![0; size as usize];
                stream.read_exact(&mut buf).await?;
                Ok(Address::Dns(String::from_utf8(buf)?))
            }
            t => Err(Errors::UnsupportedAddressType(t)),
        }?;

        let port = stream.read_u16().await?;

        Ok(CommandRequest {
            command,
            address,
            port,
        })
    }
}

#[derive(Debug)]
pub struct ConnectResponse {
    pub reply: u8,
    pub address: Address,
    pub port: u16,
}

impl From<ConnectResponse> for Vec<u8> {
    fn from(value: ConnectResponse) -> Self {
        let mut resp = vec![VERSION, value.reply, 0x0_u8];
        resp.append(&mut value.address.into());
        resp.extend_from_slice(&value.port.to_be_bytes());

        resp
    }
}

impl ConnectResponse {
    pub fn success(address: Address, port: u16) -> ConnectResponse {
        ConnectResponse {
            reply: RESP_SUCCEEDED,
            address,
            port,
        }
    }

    pub fn geneal_failure() -> ConnectResponse {
        ConnectResponse {
            reply: RESP_GENERAL_FAILURE,
            address: Ipv4Addr::UNSPECIFIED.into(),
            port: 0,
        }
    }

    pub fn network_unreachable(address: Address, port: u16) -> ConnectResponse {
        ConnectResponse {
            reply: RESP_NETWORK_UNREACHABLE,
            address,
            port,
        }
    }

    pub fn host_unreachable(address: Address, port: u16) -> ConnectResponse {
        ConnectResponse {
            reply: RESP_HOST_UNREACHABLE,
            address,
            port,
        }
    }

    pub fn connection_refused(address: Address, port: u16) -> ConnectResponse {
        ConnectResponse {
            reply: RESP_CONNECTION_REFUSED,
            address,
            port,
        }
    }

    pub fn unsupported_address() -> ConnectResponse {
        ConnectResponse {
            reply: RESP_ADDRESS_NOT_SUPPORTED,
            address: Ipv4Addr::UNSPECIFIED.into(),
            port: 0,
        }
    }

    pub(crate) fn unsupported_command() -> ConnectResponse {
        ConnectResponse {
            reply: RESP_COMMAND_NOT_SUPPORTED,
            address: Ipv4Addr::UNSPECIFIED.into(),
            port: 0,
        }
    }
}

impl From<Errors> for ConnectResponse {
    fn from(value: Errors) -> Self {
        match value {
            Errors::General(_) => ConnectResponse::geneal_failure(),
            Errors::UnsupportedCommand(_) => ConnectResponse::unsupported_command(),
            Errors::UnsupportedAddressType(_) => ConnectResponse::unsupported_address(),
        }
    }
}

#[cfg(test)]
mod tests;
