// https://www.rfc-editor.org/rfc/rfc1928

pub const VERSION: u8 = 5;

pub const AUTH_NOT_REQUIRED: u8 = 0x00;
#[allow(dead_code)]
pub const AUTH_GSSAPI: u8 = 0x01;
#[allow(dead_code)]
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
pub const RESP_DENIED: u8 = 0x02;
pub const RESP_NETWORK_UNREACHABLE: u8 = 0x03;
pub const RESP_HOST_UNREACHABLE: u8 = 0x04;
pub const RESP_CONNECTION_REFUSED: u8 = 0x05;
pub const RESP_TTL_EXPIRED: u8 = 0x06;
pub const RESP_COMMAND_NOT_SUPPORTED: u8 = 0x07;
pub const RESP_ADDRESS_NOT_SUPPORTED: u8 = 0x08;

pub struct AuthResponse {
    pub method: u8,
}

impl AuthResponse {
    pub fn not_required() -> AuthResponse {
        AuthResponse {
            method: AUTH_NOT_REQUIRED,
        }
    }

    pub fn none() -> AuthResponse {
        AuthResponse { method: AUTH_NONE }
    }

    pub fn to_buf(&self) -> [u8; 2] {
        [VERSION, self.method]
    }
}

#[derive(Debug)]
pub enum Address {
    IPv4([u8; 4]),
    IPv6([u8; 16]),
    DNS(String),
}

#[derive(Debug)]
pub struct Response {
    pub reply: u8,
    pub address: Address,
    pub port: u16,
}

impl Response {
    pub fn success(address: Address, port: u16) -> Response {
        Response {
            reply: RESP_SUCCEEDED,
            address,
            port,
        }
    }
    pub fn geneal_failure() -> Response {
        Response {
            reply: RESP_GENERAL_FAILURE,
            address: Address::IPv4([0, 0, 0, 0]),
            port: 0,
        }
    }

    pub fn unsupported_address() -> Response {
        Response {
            reply: RESP_ADDRESS_NOT_SUPPORTED,
            address: Address::IPv4([0, 0, 0, 0]),
            port: 0,
        }
    }

    pub(crate) fn unsupported_command() -> Response {
        Response {
            reply: RESP_COMMAND_NOT_SUPPORTED,
            address: Address::IPv4([0, 0, 0, 0]),
            port: 0,
        }
    }

    pub fn to_buf(&self) -> Vec<u8> {
        [
            vec![VERSION, self.reply, 0x0 as u8],
            match &self.address {
                Address::IPv4(a) => [vec![ATYPE_IPV4], Vec::from(*a)].concat(),

                Address::IPv6(a) => [vec![ATYPE_IPV6], Vec::from(*a)].concat(),
                Address::DNS(a) => {
                    [vec![ATYPE_DNS, a.len() as u8], Vec::from(a.as_bytes())].concat()
                }
            },
            Vec::from(self.port.to_be_bytes()),
        ]
        .concat()
    }
}
