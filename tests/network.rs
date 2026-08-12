use std::net::IpAddr;
use std::str::FromStr;
use tiny_wasm_runtime::io::net::{TCPListener, TcpStream};
use tiny_wasm_runtime::WasmRuntimeAsyncEngine;

async fn test_tcp_stream_connect() {
    WasmRuntimeAsyncEngine::block_on(async {
        println!("=== TcpStream Connect Test Start ===");

        let result = WasmRuntimeAsyncEngine::spawn(async move {
            let mut listener = TCPListener::new_ipv4().unwrap();
            let addr = IpAddr::from_str("0.0.0.0").expect("Invalid IP address");
            listener.bind(addr, 63000).await.unwrap();
            listener.accept().await.unwrap();
            println!("[Other] Successfully accepted to 127.0.0.1:63000.");
            let bytes = listener.read().await.unwrap();
            println!(
                "[Other] result {}",
                String::from_utf8(bytes.clone()).unwrap()
            );
            assert_eq!("Hello".as_bytes().to_vec(), bytes);
            listener.write("Bye!".as_bytes().to_vec()).await.unwrap();
            println!(
                "[Other] after write {}",
                String::from_utf8(bytes.clone()).unwrap()
            );
            listener
        });

        // Create the TCP stream
        let mut stream = TcpStream::new_ipv4().expect("Failed to create TCP stream");

        println!("[Main] Created TcpStream.");

        // Attempt to connect to localhost:8080
        let addr = IpAddr::from_str("127.0.0.1").expect("Invalid IP address");
        let connect_result = stream.connect(addr, 63000).await;

        match connect_result {
            Ok(_) => {
                println!("[Main] Successfully connected to 127.0.0.1:63000.");
                stream.write("Hello".as_bytes().to_vec()).await.unwrap();
            }
            Err(e) => {
                println!("[Main] Connection failed: {:?}", e);
                panic!("Connection attempt failed unexpectedly: {:?}", e);
            }
        }

        let _listener = result.await;
        let bytes = stream.read().await.unwrap();
        assert_eq!(bytes, "Bye!".as_bytes().to_vec());
        println!("=== TcpStream Connect Test Complete ===");
    });
}

tiny_wasm_runtime::async_command! {
    println!("test test_tcp_stream_connect ...");
    test_tcp_stream_connect().await;
    println!("test test_tcp_stream_connect ... ok");
}
