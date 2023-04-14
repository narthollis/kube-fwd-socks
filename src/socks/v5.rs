pub const VERSION: u8 = 5;

pub const AUTH_NOT_REQUIRED: u8 = 0x00;
#[allow(dead_code)]
pub const AUTH_GSSAPI: u8 = 0x01;
#[allow(dead_code)]
pub const AUTH_USER_PASS: u8 = 0x02;
pub const AUTH_NONE: u8 = 0xFF;

pub struct AuthResponse {
    pub method: u8,
}

impl AuthResponse {
    pub fn not_required() -> AuthResponse {
        AuthResponse { method: AUTH_NOT_REQUIRED }
    }
        pub fn none() -> AuthResponse {
        AuthResponse { method: AUTH_NONE }
    }
}

impl From<AuthResponse> for [u8; 2] {
    fn from(value: AuthResponse) -> Self {
        [VERSION, value.method]
    }
}


//pub enum Address {
//    IPv4([u8; 4]),
//    IPv6([u8; 16]),
//    DNS(String),
//}
//
//pub struct Response {
//    pub version: u8,
//    pub reply: u8,
//    pub address: Address,
//    pub 
//    
//    pub dest_port: u16,
//    pub dest_ip: [u8; 4],
//}
//
//impl Response {
//    pub fn granted(dest_port: u16, dest_ip: [u8; 4]) -> Response {
//        Response {
//            version: RESP_VERSION,
//            result: RESP_CODE_GRANTED,
//            dest_port,
//            dest_ip,
//        }
//    }
//    pub fn rejected_or_failed(dest_port: u16, dest_ip: [u8; 4]) -> Response {
//        Response {
//            version: RESP_VERSION,
//            result: RESP_CODE_REJECT_OR_FAILED,
//            dest_port,
//            dest_ip,
//        }
//    }
//}
//
//impl From<Response> for Vec<u8> {
//    fn from(value: Response) -> Self {
//        let p = value.dest_port.to_be_bytes();
//
//        [value.version, value.result, p[0], p[1], value.dest_ip[0], value.dest_ip[1], value.dest_ip[2], value.dest_ip[3]]
//    }
//}