use crate::{
    bindings::wasi::{
        io::{
            poll::Pollable,
            streams::{InputStream, OutputStream},
        },
        sockets::{
            instance_network::instance_network,
            network::{IpAddress, IpSocketAddress, Ipv4SocketAddress, Ipv6SocketAddress, Network},
            tcp::{IpAddressFamily, TcpSocket},
            tcp_create_socket::{create_tcp_socket, ErrorCode},
        },
    },
    engine::{NEXT_ID, REACTOR},
};
use std::io::ErrorKind;
use std::net::IpAddr;
use std::{cell::OnceCell, future::Future, sync::Arc, task::Poll};

pub struct TcpStream {
    socket: TcpSocket,
    pollable: PollableRef,
    input_stream: OnceCell<InputStream>,
    output_stream: OnceCell<OutputStream>,
    network: Network,
}
type IOResult<T> = std::io::Result<T>;
type IOError = std::io::Error;
type PollableRef = Arc<Pollable>;

struct ConnectionFuture<'a> {
    stream: &'a mut TcpStream,
    async_key: String,
    address: IpAddress,
    port: u16,
}

impl TcpStream {
    pub fn new_ipv4() -> IOResult<Self> {
        Self::new_inner(IpAddressFamily::Ipv4)
    }

    pub fn new_ipv6() -> IOResult<Self> {
        Self::new_inner(IpAddressFamily::Ipv6)
    }

    pub fn new_inner(address: IpAddressFamily) -> IOResult<Self> {
        let socket = create_tcp_socket(address)?;
        let pollable = socket.subscribe();
        Ok(Self {
            socket,
            pollable: Arc::new(pollable),
            input_stream: OnceCell::new(),
            output_stream: OnceCell::new(),
            network: instance_network(),
        })
    }
    //asynchronously connects to the ip address
    pub async fn connect<T: Into<IpAddress>>(&mut self, address: T, port: u16) -> IOResult<()> {
        let connect_future = ConnectionFuture {
            stream: self,
            async_key: format!(
                "socket-connection={}",
                NEXT_ID.load(std::sync::atomic::Ordering::Relaxed)
            ),
            address: address.into(),
            port,
        };
        connect_future.await
    }

    fn start_connect<T: Into<IpAddress>>(&mut self, address: T, port: u16) -> IOResult<()> {
        let ip_address: IpAddress = address.into();
        let socket_address = match ip_address {
            IpAddress::Ipv4(address) => IpSocketAddress::Ipv4(Ipv4SocketAddress { port, address }),
            IpAddress::Ipv6(address) => IpSocketAddress::Ipv6(Ipv6SocketAddress {
                port,
                address,
                scope_id: 0,  //need to put the right details here
                flow_info: 0, // need to put the right details here
            }),
        };
        self.socket.start_connect(&self.network, socket_address)?;
        Ok(())
    }

    pub fn finish_connecting(&mut self) -> IOResult<()> {
        let (input, output) = self.socket.finish_connect()?;
        let _ = self.input_stream.set(input);
        let _ = self.output_stream.set(output);
        Ok(())
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

impl<'a> Future for ConnectionFuture<'a> {
    type Output = IOResult<()>;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.get_mut();
        if !REACTOR.lock().unwrap().is_pollable(&this.async_key) {
            this.stream.start_connect(this.address, this.port)?;
            NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            REACTOR.lock().unwrap().register(
                this.async_key.clone(),
                (this.stream.pollable.clone(), cx.waker().clone()),
            );
        }

        //A PLACE TO CHECK IF THE REACTOR UPDATED THIS KEY
        if REACTOR.lock().unwrap().check_ready(&this.async_key) {
            this.stream.finish_connecting()?;
            Poll::Ready(Ok(()))
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
