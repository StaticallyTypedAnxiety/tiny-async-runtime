#[path = "common.rs"]
mod common;

use common::{
    parse_u16_arg, parse_usize_arg, run_directory_server, DirectoryServerConfig,
    DEFAULT_DIRECTORY_BIND_HOST, DEFAULT_DIRECTORY_PORT,
};

fn print_usage() {
    println!("usage: experiment-directory-server [bind-host] [port] [max-requests]");
    println!("example: experiment-directory-server 0.0.0.0 64000");
}

#[tiny_wasm_runtime::main]
async fn main() {
    if std::env::args().any(|argument| argument == "--help" || argument == "-h") {
        print_usage();
        return;
    }

    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let bind_host = arguments
        .first()
        .cloned()
        .unwrap_or_else(|| DEFAULT_DIRECTORY_BIND_HOST.to_string());
    let port = parse_u16_arg(arguments.get(1), "port", DEFAULT_DIRECTORY_PORT)
        .expect("directory port should be valid");
    let max_requests = match arguments.get(2) {
        Some(_) => Some(
            parse_usize_arg(arguments.get(2), "max-requests", 0)
                .expect("max-requests should be valid"),
        ),
        None => None,
    };

    run_directory_server(DirectoryServerConfig::new(
        bind_host,
        port,
        "directory-server.log",
        max_requests,
    ))
    .await
    .expect("directory server should run");
}
