mod auth_request_parse {
    use tokio_test::io;

    use super::super::*;

    #[tokio::test]
    async fn error_if_wrong_version() {
        let mut stream = io::Builder::new().read(&[0x04_u8]).build();

        let req_res = AuthRequest::parse(&mut stream).await;

        assert!(req_res.is_err());
    }

    #[tokio::test]
    async fn parse_no_auth_options() {
        let mut stream = io::Builder::new().read(&[VERSION]).read(&[0x00_u8]).build();

        let req_res = AuthRequest::parse(&mut stream).await;

        let req = req_res.unwrap();

        assert_eq!(req.requests.len(), 0);
    }

    #[tokio::test]
    async fn parse_single_auth_options() {
        let mut stream = io::Builder::new()
            .read(&[VERSION])
            .read(&[0x01_u8])
            .read(&[AUTH_NOT_REQUIRED])
            .build();

        let req_res = AuthRequest::parse(&mut stream).await;

        let req = req_res.unwrap();

        assert_eq!(req.requests, vec![AuthMethods::NotRequired]);
    }

    #[tokio::test]
    async fn parse_multiple_auth_options() {
        let mut stream = io::Builder::new()
            .read(&[VERSION])
            .read(&[0x03_u8])
            .read(&[AUTH_NOT_REQUIRED, AUTH_GSSAPI, AUTH_USER_PASS])
            .build();

        let req_res = AuthRequest::parse(&mut stream).await;

        let req = req_res.unwrap();

        assert_eq!(
            req.requests,
            vec![
                AuthMethods::NotRequired,
                AuthMethods::Gssapi,
                AuthMethods::Basic
            ]
        );
    }
}
mod address_parse {
    use tokio_test::io;

    use super::super::*;

    #[tokio::test]
    async fn error_if_wrong_version() {
        let mut stream = io::Builder::new().read(&[0x04_u8]).build();

        let req_res = AuthRequest::parse(&mut stream).await;

        assert!(req_res.is_err())
    }

    #[tokio::test]
    async fn parse_ipv4() {}
}

mod address_into_vec_u8 {
    use std::net::Ipv4Addr;

    use super::super::*;

    #[test]
    fn ipv4_address() {
        let address = Address::IpAddr(Ipv4Addr::from([192, 0, 2, 20]).into());

        let res: Vec<u8> = address.into();

        assert_eq!(res, vec![ATYPE_IPV4, 192, 0, 2, 20]);
    }

    #[test]
    fn ipv6_address() {
        let address = Address::IpAddr(
            Ipv6Addr::from([32, 1, 13, 184, 0, 0, 0, 0, 0, 19, 21, 81, 1, 51, 0, 1]).into(),
        );

        let res: Vec<u8> = address.into();

        assert_eq!(
            res,
            vec![ATYPE_IPV6, 32, 1, 13, 184, 0, 0, 0, 0, 0, 19, 21, 81, 1, 51, 0, 1]
        );
    }

    #[test]
    fn dns_address() {
        let address = Address::Dns("example.com".into());

        let res: Vec<u8> = address.into();

        assert_eq!(
            res,
            vec![ATYPE_DNS, 11, 101, 120, 97, 109, 112, 108, 101, 46, 99, 111, 109]
        );
    }
}
