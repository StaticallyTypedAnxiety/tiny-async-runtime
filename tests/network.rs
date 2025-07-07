use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener};
use std::str::FromStr;
use std::thread;
use std::time::Duration;
use tiny_wasm_runtime::io::net::TcpStream;
use tiny_wasm_runtime::{Timer, WasmRuntimeAsyncEngine};

#[test]
#[ignore = "first need support for tcp listening"]
fn test_tcp_stream_connect() {
    WasmRuntimeAsyncEngine::block_on(async {
        println!("=== TcpStream Connect Test Start ===");

        // Create the TCP stream
        let mut stream = TcpStream::new_ipv4().expect("Failed to create TCP stream");

        println!("[Main] Created TcpStream.");

        // Attempt to connect to localhost:8080
        let addr = IpAddr::from_str("127.0.0.1").expect("Invalid IP address");

        let connect_result = stream.connect(addr, 63000).await;

        match connect_result {
            Ok(_) => {
                println!("[Main] Successfully connected to 127.0.0.1:63000.");
            }
            Err(e) => {
                println!("[Main] Connection failed: {:?}", e);
                // For testing, you might still consider this a pass if the connection is refused.
                // Uncomment below if you want it to tolerate refused connections as "success."
                // if e.kind() == std::io::ErrorKind::ConnectionRefused {
                //     println!("[Main] Connection refused as expected for test.");
                //     return;
                // }
                panic!("Connection attempt failed unexpectedly: {:?}", e);
            }
        }

        // Wait a little to simulate doing work
        Timer::sleep(Duration::from_millis(200)).await;
        println!("[Main] Done waiting, TcpStream will now be dropped.");

        println!("=== TcpStream Connect Test Complete ===");
    });
}
