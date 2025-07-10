use std::{
    collections::HashMap,
    io::{BufRead, Write}, sync::{atomic::AtomicBool, Arc},
};

#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

pub trait IntoResponse {
    fn into_response(&self) -> Response;
}
impl<T: ToString> IntoResponse for T {
    fn into_response(&self) -> Response {
        Response {
            status: 200,
            body: self.to_string(),
        }
    }
}

impl IntoResponse for Response {
    fn into_response(&self) -> Response {
        self.clone()
    }
}

#[derive(Copy, Debug, Clone, Hash, PartialEq, Eq)]
pub enum RequestType {
    GET,
    POST,
}

pub trait Handler: Send + Sync {
    fn handle(&self, url: &str) -> Box<dyn IntoResponse>;
}

impl Handler for () {
    fn handle(&self, url: &str) -> Box<dyn IntoResponse> {
        Response {
            status: 404,
            body: format!("404 not found: {}", url),
        }
        .into_boxed_response()
    }
}

pub trait IntoBoxedResponse {
    fn into_boxed_response(self) -> Box<dyn IntoResponse>;
}

impl<T: IntoResponse + 'static> IntoBoxedResponse for T {
    fn into_boxed_response(self) -> Box<dyn IntoResponse> {
        Box::new(self)
    }
}

impl<F> Handler for F
where
    for<'a> F: Fn(&'a str) -> Box<dyn IntoResponse> + 'static + Sync + Send,
{
    fn handle(&self, url: &str) -> Box<dyn IntoResponse> {
        (self)(url)
    }
}

pub struct Server {
    paths: HashMap<(String, RequestType), Box<dyn Handler>>,
    thread_pool: usize,
    die: Arc<AtomicBool>,
}

const HTTP_VERSION: &str = "HTTP/1.1";

impl Server {
    pub fn new(thread_pool: usize) -> Self {
        assert!(thread_pool > 0);
        Server {
            paths: HashMap::new(),
            thread_pool,
            die: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn handle_request(&self, mut stream: std::net::TcpStream) {
        println!("new connection yay");
        let buf_stream = std::io::BufReader::new(&stream);
        println!("reading request");
        let a = buf_stream
            .lines()
            .map(|i| i.unwrap())
            .take_while(|i| !i.is_empty())
            .collect::<Vec<String>>();
        let request_info = a[0].split_whitespace().collect::<Vec<&str>>();
        let request_type = if request_info[0] == "GET" {
            RequestType::GET
        } else if request_info[0] == "POST" {
            RequestType::POST
        } else {
            println!("invalid request:\n{:#?}", request_info);
            return;
        };
        let path = request_info[1];
        if let Some(handler) = self.paths.get(&(path.to_string(), request_type)) {
            let response = handler.handle(path).into_response();
            let response_string = format_response(response);
            println!("sending response:\n{}", response_string);
            stream.write_all(response_string.as_bytes()).unwrap();
        } else {
            let response = ().handle(path).into_response();
            let response_string = format_response(response);
            println!("sending response:\n{}", response_string);
            stream.write_all(response_string.as_bytes()).unwrap();
        }
    }

    pub fn add_path<H>(&mut self, path: &str, handler: H, req_type: RequestType)
    where
        H: Handler + 'static,
    {
        self.paths
            .insert((path.to_string(), req_type), Box::new(handler));
    }

    pub fn start(self: Arc<Self>, host: &str, port: u16) {
        let listener = std::net::TcpListener::bind(format!("{}:{}", host, port)).unwrap();
        println!("Listening on http://{}:{}", host, port);

        let thread_pool = self.thread_pool;
        let mut threads = Vec::new();

        for _ in 0..thread_pool {
            let (sender, receiver) = std::sync::mpsc::channel();
            let processing_flag = Arc::new(AtomicBool::new(false));
            let cloned_processing_flag = processing_flag.clone();
            let new_server = self.clone();
            let cloned_die = self.die.clone();
            threads.push((
                std::thread::spawn(move || {
                    loop {
                        if cloned_die.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                        let stream = receiver.recv().unwrap();
                        cloned_processing_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                        new_server.handle_request(stream);
                        cloned_processing_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                    }
                }), sender, processing_flag
            ));
        }

        for stream in listener.incoming() {
            if self.die.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            match stream {
                Ok(stream) => {
                    loop {
                        if let Some(thread) = threads.iter().filter(|i| {
                            !i.2.load(std::sync::atomic::Ordering::Relaxed)
                        }).nth(0) {
                            thread.1.send(stream).unwrap();
                            break;
                        }
                    }
                }
                Err(e) => {
                    println!("Error: {}", e);
                }
            }
        }

        println!("Shutting down...");
        threads.into_iter().for_each(|i| i.0.join().unwrap());
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.die.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn format_response(response: Response) -> String {
    let response_string = format!(
        r#"{HTTP_VERSION} {}
Server: hittery (timelessnesses' own http server)
Content-Type: text/plain
Content-Length: {}

{}
            "#,
        response.status,
        response.body.len(),
        response.body
    );

    response_string.lines().map(|i| i.to_string()).collect::<Vec<String>>().join("\r\n")
}
