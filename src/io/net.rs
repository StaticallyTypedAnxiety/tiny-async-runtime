//! `TcpStream`/`TCPListener` over `wasi:sockets/types@0.3.0`'s `tcp-socket`
//! resource.

use crate::bindings::{
    wasi::sockets::types::{ErrorCode, IpAddress, IpAddressFamily, IpSocketAddress, TcpSocket},
    wit_stream,
};
use std::io::ErrorKind;
use std::net::IpAddr;
use wit_bindgen::rt::async_support::{FutureReader, StreamReader, StreamResult, StreamWriter};

type IOResult<T> = std::io::Result<T>;
type IOError = std::io::Error;
const READ_BUFFER_SIZE: usize = 4096;

pub struct InnerSocket {
    socket: TcpSocket,
    accepted_socket: Option<TcpSocket>,
    accept_stream: Option<StreamReader<TcpSocket>>,
    receive_stream: Option<StreamReader<u8>>,
    receive_completion: Option<FutureReader<Result<(), ErrorCode>>>,
    send_writer: Option<StreamWriter<u8>>,
    send_completion: Option<FutureReader<Result<(), ErrorCode>>>,
}

pub struct TcpStream {
    inner: InnerSocket,
}

pub struct TCPListener {
    inner: InnerSocket,
}

impl InnerSocket {
    fn new_inner(address: IpAddressFamily) -> IOResult<Self> {
        let socket = TcpSocket::create(address)?;
        Ok(Self {
            socket,
            accepted_socket: None,
            accept_stream: None,
            receive_stream: None,
            receive_completion: None,
            send_writer: None,
            send_completion: None,
        })
    }

    fn from_connected_socket(socket: TcpSocket) -> Self {
        Self {
            socket,
            accepted_socket: None,
            accept_stream: None,
            receive_stream: None,
            receive_completion: None,
            send_writer: None,
            send_completion: None,
        }
    }

    async fn connect<T: Into<IpAddress>>(&mut self, address: T, port: u16) -> IOResult<()> {
        let socket_address = IpSocketAddress::new(address.into(), port);
        self.socket.connect(socket_address).await?;
        Ok(())
    }

    fn bind<T: Into<IpAddress>>(&self, address: T, port: u16) -> IOResult<()> {
        let bind_address = IpSocketAddress::new(address.into(), port);
        self.socket.bind(bind_address)?;
        Ok(())
    }

    async fn accept_next(&mut self) -> IOResult<TcpSocket> {
        let stream = match &mut self.accept_stream {
            Some(stream) => stream,
            None => {
                let stream = self.socket.listen()?;
                self.accept_stream.insert(stream)
            }
        };
        stream.next().await.ok_or_else(|| {
            IOError::new(
                ErrorKind::UnexpectedEof,
                "listening socket closed while waiting to accept a connection",
            )
        })
    }

    async fn read(&mut self) -> IOResult<Vec<u8>> {
        let receive_completion = &mut self.receive_completion;
        let socket = self.accepted_socket.as_ref().unwrap_or(&self.socket);
        let reader = self.receive_stream.get_or_insert_with(|| {
            let (stream, completion) = socket.receive();
            *receive_completion = Some(completion);
            stream
        });
        let (status, bytes) = reader.read(Vec::with_capacity(READ_BUFFER_SIZE)).await;

        if matches!(status, StreamResult::Dropped) {
            if let Some(completion) = self.receive_completion.take() {
                completion.await?;
            }
        }

        Ok(bytes)
    }

    async fn write(&mut self, bytes: Vec<u8>) -> IOResult<usize> {
        let send_completion = &mut self.send_completion;
        let socket = self.accepted_socket.as_ref().unwrap_or(&self.socket);
        let writer = self.send_writer.get_or_insert_with(|| {
            let (writer, reader) = wit_stream::new::<u8>();
            *send_completion = Some(socket.send(reader));
            writer
        });
        let len = bytes.len();
        let remaining = writer.write_all(bytes).await;
        Ok(len - remaining.len())
    }
}

impl TcpStream {
    pub fn new_ipv4() -> IOResult<Self> {
        Ok(Self {
            inner: InnerSocket::new_inner(IpAddressFamily::Ipv4)?,
        })
    }

    pub fn new_ipv6() -> IOResult<Self> {
        Ok(Self {
            inner: InnerSocket::new_inner(IpAddressFamily::Ipv6)?,
        })
    }

    pub async fn connect<T: Into<IpAddress>>(&mut self, address: T, port: u16) -> IOResult<()> {
        self.inner.connect(address, port).await
    }

    pub async fn write(&mut self, bytes: Vec<u8>) -> IOResult<usize> {
        self.inner.write(bytes).await
    }

    pub async fn read(&mut self) -> IOResult<Vec<u8>> {
        self.inner.read().await
    }
}

impl TCPListener {
    pub fn new_ipv4() -> IOResult<Self> {
        Ok(Self {
            inner: InnerSocket::new_inner(IpAddressFamily::Ipv4)?,
        })
    }

    pub fn new_ipv6() -> IOResult<Self> {
        Ok(Self {
            inner: InnerSocket::new_inner(IpAddressFamily::Ipv6)?,
        })
    }

    pub async fn bind<T: Into<IpAddress>>(&mut self, address: T, port: u16) -> IOResult<()> {
        self.inner.bind(address, port)
    }

    pub async fn accept(&mut self) -> IOResult<()> {
        let socket = self.inner.accept_next().await?;
        self.inner.accepted_socket = Some(socket);
        Ok(())
    }

    pub async fn accept_stream(&mut self) -> IOResult<TcpStream> {
        let socket = self.inner.accept_next().await?;
        Ok(TcpStream {
            inner: InnerSocket::from_connected_socket(socket),
        })
    }

    pub async fn write(&mut self, bytes: Vec<u8>) -> IOResult<usize> {
        self.inner.write(bytes).await
    }

    pub async fn read(&mut self) -> IOResult<Vec<u8>> {
        self.inner.read().await
    }

    pub fn accepted(&self) -> bool {
        self.inner.accepted_socket.is_some()
    }
}

impl IpSocketAddress {
    fn new<T: Into<IpAddress>>(address: T, port: u16) -> IpSocketAddress {
        let local_address: IpAddress = address.into();
        match local_address {
            IpAddress::Ipv4(address) => {
                IpSocketAddress::Ipv4(crate::bindings::wasi::sockets::types::Ipv4SocketAddress {
                    port,
                    address,
                })
            }
            IpAddress::Ipv6(address) => {
                IpSocketAddress::Ipv6(crate::bindings::wasi::sockets::types::Ipv6SocketAddress {
                    port,
                    address,
                    scope_id: 0,
                    flow_info: 0,
                })
            }
        }
    }
}

impl From<IpAddr> for IpAddress {
    fn from(address: IpAddr) -> Self {
        match address {
            IpAddr::V4(v4) => {
                let octets = v4.octets();
                IpAddress::Ipv4((octets[0], octets[1], octets[2], octets[3]))
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

impl From<ErrorCode> for IOError {
    fn from(error_code: ErrorCode) -> Self {
        let kind = (&error_code).into();
        IOError::new(kind, error_code)
    }
}

impl From<&ErrorCode> for ErrorKind {
    fn from(error_code: &ErrorCode) -> Self {
        match error_code {
            ErrorCode::AccessDenied => ErrorKind::PermissionDenied,
            ErrorCode::NotSupported => ErrorKind::Unsupported,
            ErrorCode::InvalidArgument => ErrorKind::InvalidInput,
            ErrorCode::OutOfMemory => ErrorKind::OutOfMemory,
            ErrorCode::Timeout => ErrorKind::TimedOut,
            ErrorCode::InvalidState => ErrorKind::Other,
            ErrorCode::AddressNotBindable => ErrorKind::Other,
            ErrorCode::AddressInUse => ErrorKind::AddrInUse,
            ErrorCode::RemoteUnreachable => ErrorKind::NotFound,
            ErrorCode::ConnectionRefused => ErrorKind::ConnectionRefused,
            ErrorCode::ConnectionBroken => ErrorKind::BrokenPipe,
            ErrorCode::ConnectionReset => ErrorKind::ConnectionReset,
            ErrorCode::ConnectionAborted => ErrorKind::ConnectionAborted,
            ErrorCode::DatagramTooLarge => ErrorKind::Other,
            ErrorCode::Other(_) => ErrorKind::Other,
        }
    }
}
