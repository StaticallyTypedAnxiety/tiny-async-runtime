#![allow(dead_code)]

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tiny_wasm_runtime::io::net::{TCPListener, TcpStream};
use tiny_wasm_runtime::Timer;

pub const DEFAULT_DIRECTORY_BIND_HOST: &str = "0.0.0.0";
pub const DEFAULT_DIRECTORY_HOST: &str = "127.0.0.1";
pub const DEFAULT_DIRECTORY_PORT: u16 = 64_000;
pub const LOG_DIR: &str = "examples/chat/logs";

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Availability {
    Available,
    Busy,
}

impl Availability {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Available => "AVAILABLE",
            Self::Busy => "BUSY",
        }
    }

    fn parse(value: &str) -> io::Result<Self> {
        match value {
            "AVAILABLE" => Ok(Self::Available),
            "BUSY" => Ok(Self::Busy),
            _ => Err(invalid_input(format!(
                "unsupported availability state: {value}"
            ))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DirectoryEntry {
    pub user: String,
    pub host: String,
    pub port: u16,
    pub status: Availability,
    pub session_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DirectoryServerConfig {
    pub bind_host: String,
    pub port: u16,
    pub log_path: PathBuf,
    pub max_requests: Option<usize>,
}

impl DirectoryServerConfig {
    pub fn new(
        bind_host: impl Into<String>,
        port: u16,
        log_name: &str,
        max_requests: Option<usize>,
    ) -> Self {
        Self {
            bind_host: bind_host.into(),
            port,
            log_path: log_path(log_name),
            max_requests,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ListenerConfig {
    pub user: String,
    pub bind_host: String,
    pub advertised_host: String,
    pub port: u16,
    pub directory_host: String,
    pub directory_port: u16,
    pub reply_messages: Vec<String>,
    pub hold_duration: Duration,
    pub max_sessions: usize,
}

#[derive(Clone, Debug)]
pub struct InitiatorConfig {
    pub user: String,
    pub target_user: String,
    pub directory_host: String,
    pub directory_port: u16,
    pub message_script: Vec<String>,
    pub wait_for_availability: bool,
    pub lookup_interval: Duration,
    pub retry_connect_delay: Duration,
}

#[derive(Clone, Debug)]
struct SessionOffer {
    session_id: String,
    from: String,
    to: String,
    messages: Vec<String>,
}

#[derive(Clone, Debug)]
struct SessionReply {
    session_id: String,
    responder: String,
    messages: Vec<String>,
}

pub async fn run_directory_server(config: DirectoryServerConfig) -> io::Result<()> {
    ensure_log_dir()?;
    append_log_line(
        &config.log_path,
        &format!(
            "directory server starting on {}:{}",
            config.bind_host, config.port
        ),
    )?;

    let bind_ip = parse_ip(&config.bind_host)?;
    let mut listener = TCPListener::new_ipv4()?;
    listener.bind(bind_ip, config.port).await?;

    let mut records = HashMap::<String, DirectoryEntry>::new();
    let mut handled_requests = 0usize;

    loop {
        let mut stream = listener.accept_stream().await?;
        let request = read_text(&mut stream).await?;
        let response = handle_directory_request(&mut records, request.trim(), &config.log_path)?;
        write_text(&mut stream, &response).await?;
        // Keep short-lived directory sockets alive for the remainder of the process.
        // This avoids a known Preview 2 resource teardown edge in the current runtime.
        std::mem::forget(stream);
        handled_requests += 1;

        if response == "OK|SHUTDOWN" {
            append_log_line(
                &config.log_path,
                "directory server received shutdown command",
            )?;
            break;
        }

        if config
            .max_requests
            .map(|limit| handled_requests >= limit)
            .unwrap_or(false)
        {
            append_log_line(
                &config.log_path,
                &format!(
                    "directory server reached request limit after {handled_requests} requests"
                ),
            )?;
            break;
        }
    }

    append_log_line(&config.log_path, "directory server exiting")?;
    Ok(())
}

pub async fn run_chat_listener(config: ListenerConfig) -> io::Result<()> {
    ensure_log_dir()?;
    let listener_log = log_path(&format!("listener-{}.log", sanitize_name(&config.user)));
    append_log_line(
        &listener_log,
        &format!(
            "{} listening on {}:{} and advertising {}:{}",
            config.user, config.bind_host, config.port, config.advertised_host, config.port
        ),
    )?;

    register_directory(
        &config.directory_host,
        config.directory_port,
        &config.user,
        &config.advertised_host,
        config.port,
    )
    .await?;
    update_directory_status(
        &config.directory_host,
        config.directory_port,
        &config.user,
        Availability::Available,
        None,
    )
    .await?;

    let bind_ip = parse_ip(&config.bind_host)?;
    let mut listener = TCPListener::new_ipv4()?;
    listener.bind(bind_ip, config.port).await?;

    for session_index in 0..config.max_sessions {
        append_log_line(
            &listener_log,
            &format!(
                "{} waiting for session {} of {}",
                config.user,
                session_index + 1,
                config.max_sessions
            ),
        )?;

        let mut stream = listener.accept_stream().await?;
        let request = read_text(&mut stream).await?;
        let offer = SessionOffer::decode(request.trim())?;
        let session_log = session_log_path(&offer.session_id);

        update_directory_status(
            &config.directory_host,
            config.directory_port,
            &config.user,
            Availability::Busy,
            Some(&offer.session_id),
        )
        .await?;

        append_log_line(
            &session_log,
            &format!(
                "{} accepted session {} from {} to {}",
                config.user, offer.session_id, offer.from, offer.to
            ),
        )?;

        for message in &offer.messages {
            append_log_line(
                &session_log,
                &format!("{} received message: {}", config.user, message),
            )?;
        }

        if !config.hold_duration.is_zero() {
            append_log_line(
                &session_log,
                &format!(
                    "{} holding session for {} ms",
                    config.user,
                    config.hold_duration.as_millis()
                ),
            )?;
            Timer::sleep(config.hold_duration).await;
        }

        let reply_messages = if config.reply_messages.is_empty() {
            vec![format!(
                "{} received {} message(s) from {}",
                config.user,
                offer.messages.len(),
                offer.from
            )]
        } else {
            config.reply_messages.clone()
        };

        let reply = SessionReply {
            session_id: offer.session_id.clone(),
            responder: config.user.clone(),
            messages: reply_messages,
        };

        for message in &reply.messages {
            append_log_line(
                &session_log,
                &format!("{} sent reply: {}", config.user, message),
            )?;
        }

        write_text(&mut stream, &reply.encode()).await?;
        update_directory_status(
            &config.directory_host,
            config.directory_port,
            &config.user,
            Availability::Available,
            None,
        )
        .await?;
    }

    append_log_line(&listener_log, &format!("{} listener exiting", config.user))?;
    Ok(())
}

pub async fn run_chat_initiator(config: InitiatorConfig) -> io::Result<String> {
    ensure_log_dir()?;
    let initiator_log = log_path(&format!("initiator-{}.log", sanitize_name(&config.user)));
    append_log_line(
        &initiator_log,
        &format!(
            "{} looking up {} through {}:{}",
            config.user, config.target_user, config.directory_host, config.directory_port
        ),
    )?;

    loop {
        let entry = lookup_directory(
            &config.directory_host,
            config.directory_port,
            &config.target_user,
        )
        .await?;

        match entry {
            Some(found) if found.status == Availability::Available => {
                let session_id = next_session_id(&config.user, &config.target_user);
                let session_log = session_log_path(&session_id);
                append_log_line(
                    &session_log,
                    &format!(
                        "{} found {} at {}:{} and is opening session {}",
                        config.user, found.user, found.host, found.port, session_id
                    ),
                )?;

                match connect_and_exchange(&config, &found, &session_id, &session_log).await {
                    Ok(()) => return Ok(session_id),
                    Err(error) if config.wait_for_availability => {
                        append_log_line(
                            &initiator_log,
                            &format!(
                                "{} failed to connect to {} for session {}: {}",
                                config.user, config.target_user, session_id, error
                            ),
                        )?;
                        Timer::sleep(config.retry_connect_delay).await;
                    }
                    Err(error) => return Err(error),
                }
            }
            Some(found) => {
                append_log_line(
                    &initiator_log,
                    &format!(
                        "{} found {} but the user is {}{}",
                        config.user,
                        found.user,
                        found.status.as_str(),
                        found
                            .session_id
                            .as_ref()
                            .map(|id| format!(" in {}", id))
                            .unwrap_or_default()
                    ),
                )?;
                if !config.wait_for_availability {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        format!("{} is busy right now", config.target_user),
                    ));
                }
                Timer::sleep(config.lookup_interval).await;
            }
            None => {
                append_log_line(
                    &initiator_log,
                    &format!(
                        "{} could not find {} in the directory",
                        config.user, config.target_user
                    ),
                )?;
                if !config.wait_for_availability {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("{} is not registered", config.target_user),
                    ));
                }
                Timer::sleep(config.lookup_interval).await;
            }
        }
    }
}

pub async fn send_directory_shutdown(directory_host: &str, directory_port: u16) -> io::Result<()> {
    let response = send_directory_request(directory_host, directory_port, "SHUTDOWN").await?;
    if response == "OK|SHUTDOWN" {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "unexpected shutdown response: {response}"
        )))
    }
}

pub fn default_messages(user: &str, target_user: &str) -> Vec<String> {
    vec![
        format!("hello from {user}"),
        format!("checking in with {target_user}"),
    ]
}

pub fn parse_message_script(raw: Option<&String>) -> Vec<String> {
    raw.map(|value| {
        value
            .split('|')
            .map(str::trim)
            .filter(|message| !message.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>()
    })
    .filter(|messages| !messages.is_empty())
    .unwrap_or_default()
}

pub fn parse_u16_arg(value: Option<&String>, name: &str, default: u16) -> io::Result<u16> {
    match value {
        Some(raw) => raw
            .parse::<u16>()
            .map_err(|error| invalid_input(format!("invalid {name} value {raw}: {error}"))),
        None => Ok(default),
    }
}

pub fn parse_usize_arg(value: Option<&String>, name: &str, default: usize) -> io::Result<usize> {
    match value {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|error| invalid_input(format!("invalid {name} value {raw}: {error}"))),
        None => Ok(default),
    }
}

pub fn parse_u64_arg(value: Option<&String>, name: &str, default: u64) -> io::Result<u64> {
    match value {
        Some(raw) => raw
            .parse::<u64>()
            .map_err(|error| invalid_input(format!("invalid {name} value {raw}: {error}"))),
        None => Ok(default),
    }
}

pub fn log_path(name: &str) -> PathBuf {
    PathBuf::from(LOG_DIR).join(name)
}

pub fn session_log_path(session_id: &str) -> PathBuf {
    log_path(&format!("session-{}.log", sanitize_name(session_id)))
}

fn ensure_log_dir() -> io::Result<()> {
    fs::create_dir_all(LOG_DIR)
}

fn append_log_line(path: &Path, line: &str) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "[{}] {line}", current_timestamp())
}

fn current_timestamp() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().to_string(),
        Err(_) => "0".to_string(),
    }
}

fn sanitize_name(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => character,
            _ => '_',
        })
        .collect()
}

fn parse_ip(host: &str) -> io::Result<IpAddr> {
    IpAddr::from_str(host)
        .map_err(|error| invalid_input(format!("invalid IPv4/IPv6 address {host}: {error}")))
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

async fn connect_and_exchange(
    config: &InitiatorConfig,
    directory_entry: &DirectoryEntry,
    session_id: &str,
    session_log: &Path,
) -> io::Result<()> {
    let offer = SessionOffer {
        session_id: session_id.to_string(),
        from: config.user.clone(),
        to: config.target_user.clone(),
        messages: if config.message_script.is_empty() {
            default_messages(&config.user, &config.target_user)
        } else {
            config.message_script.clone()
        },
    };

    let mut stream = TcpStream::new_ipv4()?;
    stream
        .connect(parse_ip(&directory_entry.host)?, directory_entry.port)
        .await?;
    write_text(&mut stream, &offer.encode()).await?;

    for message in &offer.messages {
        append_log_line(
            session_log,
            &format!("{} sent message: {}", config.user, message),
        )?;
    }

    let reply = SessionReply::decode(read_text(&mut stream).await?.trim())?;
    for message in &reply.messages {
        append_log_line(
            session_log,
            &format!("{} received reply: {}", config.user, message),
        )?;
    }

    Ok(())
}

fn next_session_id(from: &str, to: &str) -> String {
    let index = SESSION_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!(
        "{}-{}-{}-{}",
        sanitize_name(from),
        sanitize_name(to),
        current_timestamp(),
        index
    )
}

async fn register_directory(
    directory_host: &str,
    directory_port: u16,
    user: &str,
    host: &str,
    port: u16,
) -> io::Result<()> {
    let response = send_directory_request(
        directory_host,
        directory_port,
        &format!("REGISTER|{user}|{host}|{port}"),
    )
    .await?;

    if response.starts_with("OK|") {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "unexpected register response: {response}"
        )))
    }
}

async fn update_directory_status(
    directory_host: &str,
    directory_port: u16,
    user: &str,
    status: Availability,
    session_id: Option<&str>,
) -> io::Result<()> {
    let session_id = session_id.unwrap_or("-");
    let response = send_directory_request(
        directory_host,
        directory_port,
        &format!("STATUS|{user}|{}|{session_id}", status.as_str()),
    )
    .await?;

    if response.starts_with("OK|") {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "unexpected status response: {response}"
        )))
    }
}

async fn lookup_directory(
    directory_host: &str,
    directory_port: u16,
    user: &str,
) -> io::Result<Option<DirectoryEntry>> {
    let response =
        send_directory_request(directory_host, directory_port, &format!("LOOKUP|{user}")).await?;

    if response.starts_with("MISSING|") {
        return Ok(None);
    }

    if !response.starts_with("FOUND|") {
        return Err(io::Error::other(format!(
            "unexpected lookup response: {response}"
        )));
    }

    let parts = response.split('|').collect::<Vec<_>>();
    if parts.len() != 6 {
        return Err(invalid_input(format!(
            "malformed lookup response with {} fields",
            parts.len()
        )));
    }

    let port = parts[3]
        .parse::<u16>()
        .map_err(|error| invalid_input(format!("invalid port {}: {error}", parts[3])))?;
    let status = Availability::parse(parts[4])?;
    let session_id = if parts[5] == "-" {
        None
    } else {
        Some(parts[5].to_string())
    };

    Ok(Some(DirectoryEntry {
        user: parts[1].to_string(),
        host: parts[2].to_string(),
        port,
        status,
        session_id,
    }))
}

async fn send_directory_request(
    directory_host: &str,
    directory_port: u16,
    request: &str,
) -> io::Result<String> {
    let mut stream = TcpStream::new_ipv4()?;
    stream
        .connect(parse_ip(directory_host)?, directory_port)
        .await?;
    write_text(&mut stream, request).await?;
    let response = read_text(&mut stream).await?;
    // Keep short-lived directory sockets alive for the remainder of the process.
    // This avoids a known Preview 2 resource teardown edge in the current runtime.
    std::mem::forget(stream);
    Ok(response)
}

fn handle_directory_request(
    records: &mut HashMap<String, DirectoryEntry>,
    request: &str,
    log_path: &Path,
) -> io::Result<String> {
    let parts = request.split('|').collect::<Vec<_>>();
    if parts.is_empty() {
        return Ok("ERROR|empty request".to_string());
    }

    match parts[0] {
        "REGISTER" => {
            if parts.len() != 4 {
                return Ok("ERROR|REGISTER expects 3 fields".to_string());
            }

            let port = parts[3]
                .parse::<u16>()
                .map_err(|error| invalid_input(format!("invalid port {}: {error}", parts[3])))?;
            let entry = DirectoryEntry {
                user: parts[1].to_string(),
                host: parts[2].to_string(),
                port,
                status: Availability::Available,
                session_id: None,
            };
            append_log_line(
                log_path,
                &format!(
                    "directory registered {} at {}:{}",
                    entry.user, entry.host, entry.port
                ),
            )?;
            records.insert(entry.user.clone(), entry);
            Ok("OK|REGISTERED".to_string())
        }
        "STATUS" => {
            if parts.len() != 4 {
                return Ok("ERROR|STATUS expects 3 fields".to_string());
            }

            let Some(entry) = records.get_mut(parts[1]) else {
                return Ok(format!("ERROR|{} is not registered", parts[1]));
            };

            entry.status = Availability::parse(parts[2])?;
            entry.session_id = if parts[3] == "-" {
                None
            } else {
                Some(parts[3].to_string())
            };
            append_log_line(
                log_path,
                &format!(
                    "directory updated {} to {}{}",
                    entry.user,
                    entry.status.as_str(),
                    entry
                        .session_id
                        .as_ref()
                        .map(|id| format!(" in {id}"))
                        .unwrap_or_default()
                ),
            )?;
            Ok("OK|STATUS".to_string())
        }
        "LOOKUP" => {
            if parts.len() != 2 {
                return Ok("ERROR|LOOKUP expects 1 field".to_string());
            }

            match records.get(parts[1]) {
                Some(entry) => Ok(format!(
                    "FOUND|{}|{}|{}|{}|{}",
                    entry.user,
                    entry.host,
                    entry.port,
                    entry.status.as_str(),
                    entry.session_id.as_deref().unwrap_or("-")
                )),
                None => Ok(format!("MISSING|{}", parts[1])),
            }
        }
        "SHUTDOWN" => Ok("OK|SHUTDOWN".to_string()),
        other => Ok(format!("ERROR|unsupported command {other}")),
    }
}

async fn read_text(stream: &mut TcpStream) -> io::Result<String> {
    let bytes = stream.read().await?;
    String::from_utf8(bytes)
        .map_err(|error| invalid_input(format!("invalid utf-8 payload: {error}")))
}

async fn write_text(stream: &mut TcpStream, text: &str) -> io::Result<()> {
    stream.write(text.as_bytes().to_vec()).await?;
    Ok(())
}

impl SessionOffer {
    fn encode(&self) -> String {
        format!(
            "SESSION|{}|{}|{}|{}",
            self.session_id,
            self.from,
            self.to,
            encode_messages(&self.messages)
        )
    }

    fn decode(payload: &str) -> io::Result<Self> {
        let parts = payload.splitn(5, '|').collect::<Vec<_>>();
        if parts.len() != 5 || parts[0] != "SESSION" {
            return Err(invalid_input(format!("malformed session offer: {payload}")));
        }

        Ok(Self {
            session_id: parts[1].to_string(),
            from: parts[2].to_string(),
            to: parts[3].to_string(),
            messages: decode_messages(parts[4]),
        })
    }
}

impl SessionReply {
    fn encode(&self) -> String {
        format!(
            "SESSION-ACK|{}|{}|{}",
            self.session_id,
            self.responder,
            encode_messages(&self.messages)
        )
    }

    fn decode(payload: &str) -> io::Result<Self> {
        let parts = payload.splitn(4, '|').collect::<Vec<_>>();
        if parts.len() != 4 || parts[0] != "SESSION-ACK" {
            return Err(invalid_input(format!("malformed session reply: {payload}")));
        }

        Ok(Self {
            session_id: parts[1].to_string(),
            responder: parts[2].to_string(),
            messages: decode_messages(parts[3]),
        })
    }
}

fn encode_messages(messages: &[String]) -> String {
    messages.join("~")
}

fn decode_messages(payload: &str) -> Vec<String> {
    payload
        .split('~')
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
