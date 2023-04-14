// https://www.openssh.com/txt/socks4.protocol
// https://www.openssh.com/txt/socks4a.protocol

pub const VERSION: u8 = 4;

pub const SOCKS4A_ADDRESS: [u8; 4] = [0, 0, 0, 1];

const RESP_VERSION: u8 = 0;
const RESP_CODE_GRANTED: u8 = 90; 
const RESP_CODE_REJECT_OR_FAILED: u8 = 91;

pub struct Response {
    pub version: u8,
    pub result: u8,
    pub dest_port: u16,
    pub dest_ip: [u8; 4],
}

impl Response {
    pub fn granted(dest_port: u16, dest_ip: [u8; 4]) -> Response {
        Response {
            version: RESP_VERSION,
            result: RESP_CODE_GRANTED,
            dest_port,
            dest_ip,
        }
    }
        pub fn rejected_or_failed(dest_port: u16, dest_ip: [u8; 4]) -> Response {
        Response {
            version: RESP_VERSION,
            result: RESP_CODE_REJECT_OR_FAILED,
            dest_port,
            dest_ip,
        }
    }
}

impl<'a> From<&'a Response> for &'a [u8] {
    fn from(value: &'a Response) -> Self {
        let p = value.dest_port.to_be_bytes();
        &[value.version, value.result, p[0], p[1], value.dest_ip[0], value.dest_ip[1], value.dest_ip[2], value.dest_ip[3]]
    }
}
