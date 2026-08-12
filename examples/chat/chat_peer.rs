#[path = "common.rs"]
mod common;

use std::time::Duration;

use common::{
    parse_message_script, parse_u16_arg, parse_u64_arg, parse_usize_arg, run_chat_initiator,
    run_chat_listener, InitiatorConfig, ListenerConfig, DEFAULT_DIRECTORY_BIND_HOST,
    DEFAULT_DIRECTORY_HOST, DEFAULT_DIRECTORY_PORT,
};

fn print_usage() {
    println!("usage:");
    println!(
        "  experiment-chat-peer listen <user> <port> [reply-script] [max-sessions] [hold-ms] [directory-host] [directory-port] [bind-host] [advertised-host]"
    );
    println!(
        "  experiment-chat-peer dial <user> <target-user> [message-script] [wait|nowait] [directory-host] [directory-port] [lookup-ms] [retry-ms]"
    );
    println!("examples:");
    println!("  experiment-chat-peer listen bob 64100 \"bob here|talk soon\" 2 1200");
    println!("  experiment-chat-peer dial alice bob \"hello bob|checking in\" wait");
}

#[tiny_wasm_runtime::main]
async fn main() {
    if std::env::args().any(|argument| argument == "--help" || argument == "-h") {
        print_usage();
        return;
    }

    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let Some(mode) = arguments.first().map(String::as_str) else {
        print_usage();
        return;
    };

    match mode {
        "listen" => {
            if arguments.len() < 3 {
                print_usage();
                return;
            }

            let user = arguments[1].clone();
            let port = parse_u16_arg(arguments.get(2), "port", 64_100)
                .expect("listener port should be valid");
            let replies = parse_message_script(arguments.get(3));
            let max_sessions = parse_usize_arg(arguments.get(4), "max-sessions", 1)
                .expect("max-sessions should be valid");
            let hold_ms =
                parse_u64_arg(arguments.get(5), "hold-ms", 1_000).expect("hold-ms should be valid");
            let directory_host = arguments
                .get(6)
                .cloned()
                .unwrap_or_else(|| DEFAULT_DIRECTORY_HOST.to_string());
            let directory_port =
                parse_u16_arg(arguments.get(7), "directory-port", DEFAULT_DIRECTORY_PORT)
                    .expect("directory-port should be valid");
            let bind_host = arguments
                .get(8)
                .cloned()
                .unwrap_or_else(|| DEFAULT_DIRECTORY_BIND_HOST.to_string());
            let advertised_host = arguments
                .get(9)
                .cloned()
                .unwrap_or_else(|| DEFAULT_DIRECTORY_HOST.to_string());

            run_chat_listener(ListenerConfig {
                user,
                bind_host,
                advertised_host,
                port,
                directory_host,
                directory_port,
                reply_messages: replies,
                hold_duration: Duration::from_millis(hold_ms),
                max_sessions,
            })
            .await
            .expect("listener should run");
        }
        "dial" => {
            if arguments.len() < 3 {
                print_usage();
                return;
            }

            let user = arguments[1].clone();
            let target_user = arguments[2].clone();
            let messages = parse_message_script(arguments.get(3));
            let wait_for_availability = arguments
                .get(4)
                .map(|value| value.eq_ignore_ascii_case("wait"))
                .unwrap_or(true);
            let directory_host = arguments
                .get(5)
                .cloned()
                .unwrap_or_else(|| DEFAULT_DIRECTORY_HOST.to_string());
            let directory_port =
                parse_u16_arg(arguments.get(6), "directory-port", DEFAULT_DIRECTORY_PORT)
                    .expect("directory-port should be valid");
            let lookup_ms = parse_u64_arg(arguments.get(7), "lookup-ms", 250)
                .expect("lookup-ms should be valid");
            let retry_ms =
                parse_u64_arg(arguments.get(8), "retry-ms", 150).expect("retry-ms should be valid");

            let session_id = run_chat_initiator(InitiatorConfig {
                user,
                target_user,
                directory_host,
                directory_port,
                message_script: messages,
                wait_for_availability,
                lookup_interval: Duration::from_millis(lookup_ms),
                retry_connect_delay: Duration::from_millis(retry_ms),
            })
            .await
            .expect("initiator should complete a chat session");

            println!("chat session completed: {session_id}");
        }
        _ => {
            print_usage();
        }
    }
}
