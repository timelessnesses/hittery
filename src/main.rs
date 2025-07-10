use std::sync::Arc;

use hittery::{IntoBoxedResponse, RequestType, Server};

fn main() {
    let mut server = Server::new(10);

    server.add_path("/", (), RequestType::GET);

    server.add_path("/hello", hello_world, RequestType::GET);

    let arced = Arc::new(server);
    arced.start("127.0.0.1", 8080);
}

fn hello_world(_url: &str) -> Box<dyn hittery::IntoResponse> {
    "Hello, world!".to_string().into_boxed_response()
}
