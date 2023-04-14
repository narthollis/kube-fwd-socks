// https://www.openssh.com/txt/socks4.protocol
// https://www.openssh.com/txt/socks4a.protocol

pub const VERSION: u8 = 4;

pub const METHOD_CONNECT: u8 = 1;
pub const METHOD_BIND: u8 = 2;

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

    pub fn to_buf(&self) -> [u8; 8] {
        let p = self.dest_port.to_be_bytes();
        [
            self.version,
            self.result,
            p[0],
            p[1],
            self.dest_ip[0],
            self.dest_ip[1],
            self.dest_ip[2],
            self.dest_ip[3],
        ]
    }
}
