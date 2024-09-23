use crate::bindings::wasi::sockets::{
    network::IpAddress,
    tcp::{IpAddressFamily, TcpSocket},
    tcp_create_socket::{create_tcp_socket, ErrorCode},
};
use std::io::ErrorKind;
use std::net::IpAddr;
struct TcpStream {
    _socket: TcpSocket,
}
type IOResult<T> = std::io::Result<T>;
type IOError = std::io::Error;

impl TcpStream {
    pub fn create<T: Into<IpAddressFamily>>(ip_address: T) -> IOResult<Self> {
        Ok(Self {
            _socket: create_tcp_socket(ip_address.into())?,
        })
    }
}

impl From<IpAddr> for IpAddress {
    fn from(address: IpAddr) -> Self {
        match address {
            IpAddr::V4(v4) => {
                let ocets = v4.octets();
                IpAddress::Ipv4((ocets[0], ocets[1], ocets[2], ocets[3]))
            }
            IpAddr::V6(v6) => {
                let segments = v6.segments();
                IpAddress::Ipv6((
                    segments[0],
                    segments[1],
                    segments[2],
                    segments[3],
                    segments[4],
                    segments[5],
                    segments[6],
                    segments[7],
                ))
            }
        }
    }
}

impl From<IpAddr> for IpAddressFamily {
    fn from(address: IpAddr) -> Self {
        if address.is_ipv4() {
            IpAddressFamily::Ipv4
        } else {
            IpAddressFamily::Ipv6
        }
    }
}

impl From<ErrorCode> for IOError {
    fn from(address: ErrorCode) -> Self {
        let kind = (&address).into();
        IOError::new(kind, address)
    }
}

impl From<&ErrorCode> for ErrorKind {
    fn from(address: &ErrorCode) -> Self {
        match address {
            ErrorCode::Unknown => ErrorKind::Other,
            ErrorCode::AccessDenied => ErrorKind::PermissionDenied,
            ErrorCode::NotSupported => ErrorKind::Unsupported,
            ErrorCode::InvalidArgument => ErrorKind::InvalidInput,
            ErrorCode::OutOfMemory => ErrorKind::OutOfMemory,
            ErrorCode::Timeout => ErrorKind::TimedOut,
            ErrorCode::ConcurrencyConflict => ErrorKind::Other,
            ErrorCode::NotInProgress => ErrorKind::Other,
            ErrorCode::WouldBlock => ErrorKind::WouldBlock,
            ErrorCode::InvalidState => ErrorKind::Other,
            ErrorCode::NewSocketLimit => ErrorKind::Other,
            ErrorCode::AddressNotBindable => ErrorKind::Other,
            ErrorCode::AddressInUse => ErrorKind::AddrInUse,
            ErrorCode::RemoteUnreachable => ErrorKind::NotFound,
            ErrorCode::ConnectionRefused => ErrorKind::ConnectionRefused,
            ErrorCode::ConnectionReset => ErrorKind::ConnectionReset,
            ErrorCode::ConnectionAborted => ErrorKind::ConnectionAborted,
            ErrorCode::DatagramTooLarge => ErrorKind::Other,
            ErrorCode::NameUnresolvable => ErrorKind::Other,
            ErrorCode::TemporaryResolverFailure => ErrorKind::Other,
            ErrorCode::PermanentResolverFailure => ErrorKind::Other,
        }
    }
}
