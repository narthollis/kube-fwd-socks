// https://www.rfc-editor.org/rfc/rfc1928

use anyhow::Ok;
use int_enum::IntEnum;

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
#[allow(dead_code)]
pub const RESP_GENERAL_FAILURE: u8 = 0x01;
#[allow(dead_code)]
pub const RESP_DENIED: u8 = 0x02;
#[allow(dead_code)]
pub const RESP_NETWORK_UNREACHABLE: u8 = 0x03;
#[allow(dead_code)]
pub const RESP_HOST_UNREACHABLE: u8 = 0x04;
#[allow(dead_code)]
pub const RESP_CONNECTION_REFUSED: u8 = 0x05;
#[allow(dead_code)]
pub const RESP_TTL_EXPIRED: u8 = 0x06;
pub const RESP_COMMAND_NOT_SUPPORTED: u8 = 0x07;
pub const RESP_ADDRESS_NOT_SUPPORTED: u8 = 0x08;

#[derive(Debug, thiserror::Error)]
pub enum Errors {
    #[error("Unsupported Command {0}")]
    UnsupportedCommand(u8),
    #[error("Unsupported Address Type {0}")]
    UnsupportedAddressType(u8),
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
    async fn parse(stream: &mut (impl tokio::io::AsyncReadExt + Unpin)) -> anyhow::Result<Self>
    where
        Self: std::marker::Sized,
    {
        let method_count = stream.read_u8().await?;
        let mut buf: Vec<u8> = vec![0; method_count as usize];
        stream.read_exact(&mut buf).await?;

        let requests = buf
            .into_iter()
            .filter_map(|v| AuthMethods::from_int(v).ok())
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
    IPv4([u8; 4]),
    IPv6([u8; 16]),
    Dns(String),
}

impl From<Address> for Vec<u8> {
    fn from(value: Address) -> Self {
        match value {
            Address::IPv4(a) => [vec![ATYPE_IPV4], Vec::from(a)].concat(),

            Address::IPv6(a) => [vec![ATYPE_IPV6], Vec::from(a)].concat(),
            Address::Dns(a) => [vec![ATYPE_DNS, a.len() as u8], Vec::from(a.as_bytes())].concat(),
        }
    }
}

#[derive(Debug)]
pub struct CommandRequest {
    pub command: Command,
    pub address: Address,
    pub port: u16,
}

impl Request for CommandRequest {
    async fn parse(stream: &mut (impl tokio::io::AsyncReadExt + Unpin)) -> anyhow::Result<Self>
    where
        Self: std::marker::Sized,
    {
        let ver = stream.read_u8().await?;
        if ver != VERSION {
            return Err(super::Errors::UnsupportedVersion(ver).into());
        }

        let command = Command::from_int(stream.read_u8().await?)
            .map_err(|e| Errors::UnsupportedCommand(e.value()))?;

        // This next byte is very literally a unused reserved byte, just read and discard
        let _rsv = stream.read_u8().await?;

        let atype = stream.read_u8().await?;
        let address = match atype {
            ATYPE_IPV4 => {
                let mut addr = [0; 4];
                stream.read_exact(&mut addr).await?;
                Ok(Address::IPv4(addr))
            }
            ATYPE_IPV6 => {
                let mut addr = [0; 16];
                stream.read_exact(&mut addr).await?;
                Ok(Address::IPv6(addr))
            }
            ATYPE_DNS => {
                let size = stream.read_u8().await?;
                let mut buf = vec![0; size as usize];
                stream.read_exact(&mut buf).await?;

                Ok(Address::Dns(String::from_utf8(buf)?))
            }
            t => Err(Errors::UnsupportedAddressType(t).into()),
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
    #[allow(dead_code)]
    pub fn geneal_failure() -> ConnectResponse {
        ConnectResponse {
            reply: RESP_GENERAL_FAILURE,
            address: Address::IPv4([0, 0, 0, 0]),
            port: 0,
        }
    }

    pub fn unsupported_address() -> ConnectResponse {
        ConnectResponse {
            reply: RESP_ADDRESS_NOT_SUPPORTED,
            address: Address::IPv4([0, 0, 0, 0]),
            port: 0,
        }
    }

    pub(crate) fn unsupported_command() -> ConnectResponse {
        ConnectResponse {
            reply: RESP_COMMAND_NOT_SUPPORTED,
            address: Address::IPv4([0, 0, 0, 0]),
            port: 0,
        }
    }
}

impl From<&Errors> for ConnectResponse {
    fn from(value: &Errors) -> Self {
        match value {
            Errors::UnsupportedCommand(_) => ConnectResponse::unsupported_command(),
            Errors::UnsupportedAddressType(_) => ConnectResponse::unsupported_address(),
        }
    }
}
