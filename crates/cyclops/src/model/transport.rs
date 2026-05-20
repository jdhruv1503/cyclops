use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::{ACCEPT_ENCODING, CONNECTION};
use hyper::{Method, Request, Response, Uri};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use crate::{CyclopsError, Result};

pub type RequestBody = Full<Bytes>;
type HyperClient = Client<HttpsConnector<HttpConnector>, RequestBody>;

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(90);
const DEFAULT_POOL_MAX_IDLE_PER_HOST: usize = 4;
const DEFAULT_RESPONSE_HEADER_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportConfig {
    pub connect_timeout: Duration,
    pub pool_idle_timeout: Duration,
    pub pool_max_idle_per_host: usize,
    pub response_header_timeout: Duration,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            pool_idle_timeout: DEFAULT_POOL_IDLE_TIMEOUT,
            pool_max_idle_per_host: DEFAULT_POOL_MAX_IDLE_PER_HOST,
            response_header_timeout: DEFAULT_RESPONSE_HEADER_TIMEOUT,
        }
    }
}

#[derive(Clone)]
pub struct HttpTransport {
    client: HyperClient,
    response_header_timeout: Duration,
}

impl HttpTransport {
    pub fn new() -> Result<Self> {
        Self::with_config(TransportConfig::default())
    }

    pub fn with_config(config: TransportConfig) -> Result<Self> {
        let mut http = HttpConnector::new();
        http.set_connect_timeout(Some(config.connect_timeout));
        http.set_keepalive(Some(config.pool_idle_timeout));
        http.enforce_http(false);

        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .map_err(|error| CyclopsError::Model(format!("failed to load TLS roots: {error}")))?
            .https_or_http()
            .enable_http1()
            .wrap_connector(http);

        let client = Client::builder(TokioExecutor::new())
            .pool_idle_timeout(config.pool_idle_timeout)
            .pool_max_idle_per_host(config.pool_max_idle_per_host)
            .http1_writev(true)
            .http1_preserve_header_case(false)
            .build(connector);

        Ok(Self {
            client,
            response_header_timeout: config.response_header_timeout,
        })
    }

    pub async fn get(&self, uri: Uri) -> Result<Response<Incoming>> {
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Full::new(Bytes::new()))
            .map_err(|error| {
                CyclopsError::Model(format!("failed to build GET request: {error}"))
            })?;

        self.execute(request).await
    }

    pub async fn post(
        &self,
        uri: Uri,
        body: impl Into<Bytes>,
        content_type: &'static str,
    ) -> Result<Response<Incoming>> {
        let request = Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header(hyper::header::CONTENT_TYPE, content_type)
            .body(Full::new(body.into()))
            .map_err(|error| {
                CyclopsError::Model(format!("failed to build POST request: {error}"))
            })?;

        self.execute(request).await
    }

    pub async fn execute(&self, request: Request<RequestBody>) -> Result<Response<Incoming>> {
        let request = with_transport_headers(request);
        tokio::time::timeout(self.response_header_timeout, self.client.request(request))
            .await
            .map_err(|_| {
                CyclopsError::Model(format!(
                    "HTTP response headers timed out after {:?}",
                    self.response_header_timeout
                ))
            })?
            .map_err(|error| CyclopsError::Model(format!("HTTP request failed: {error}")).into())
    }

    pub async fn execute_collect(&self, request: Request<RequestBody>) -> Result<Response<Bytes>> {
        let (parts, body) = self.execute(request).await?.into_parts();
        let body = body
            .collect()
            .await
            .map_err(|error| CyclopsError::Model(format!("HTTP response read failed: {error}")))?
            .to_bytes();

        Ok(Response::from_parts(parts, body))
    }
}

pub fn empty_body() -> RequestBody {
    Full::new(Bytes::new())
}

pub fn bytes_body(body: impl Into<Bytes>) -> RequestBody {
    Full::new(body.into())
}

fn with_transport_headers(mut request: Request<RequestBody>) -> Request<RequestBody> {
    let headers = request.headers_mut();
    headers.insert(
        ACCEPT_ENCODING,
        hyper::header::HeaderValue::from_static("identity"),
    );
    headers.insert(
        CONNECTION,
        hyper::header::HeaderValue::from_static("keep-alive"),
    );

    request
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use hyper::StatusCode;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use super::*;

    #[tokio::test]
    async fn get_collects_response_from_local_http_server() {
        let server = LocalHttpServer::spawn(ResponseSpec {
            status: "200 OK",
            body: "hello from tcp",
        })
        .await;
        let transport = HttpTransport::new().unwrap();

        let response = transport.get(server.uri("/health")).await.unwrap();
        let (parts, body) = response.into_parts();
        let body = body.collect().await.unwrap().to_bytes();
        let observed = server.join().await;

        assert_eq!(parts.status, StatusCode::OK);
        assert_eq!(body, Bytes::from_static(b"hello from tcp"));
        assert_eq!(observed.method, "GET");
        assert_eq!(observed.path, "/health");
        assert_eq!(observed.version, "HTTP/1.1");
        assert_eq!(
            observed.headers.get("accept-encoding").map(String::as_str),
            Some("identity")
        );
        assert_eq!(
            observed.headers.get("connection").map(String::as_str),
            Some("keep-alive")
        );
        assert!(observed.body.is_empty());
    }

    #[tokio::test]
    async fn post_sends_body_to_local_http_server() {
        let server = LocalHttpServer::spawn(ResponseSpec {
            status: "201 Created",
            body: "created",
        })
        .await;
        let transport = HttpTransport::new().unwrap();
        let body = Bytes::from_static(br#"{"ping":true}"#);

        let response = transport
            .post(server.uri("/echo"), body.clone(), "application/json")
            .await
            .unwrap();
        let response_body = response.into_body().collect().await.unwrap().to_bytes();
        let observed = server.join().await;

        assert_eq!(response_body, Bytes::from_static(b"created"));
        assert_eq!(observed.method, "POST");
        assert_eq!(observed.path, "/echo");
        assert_eq!(observed.version, "HTTP/1.1");
        assert_eq!(
            observed.headers.get("content-type").map(String::as_str),
            Some("application/json")
        );
        assert_eq!(observed.body, body);
    }

    struct ResponseSpec {
        status: &'static str,
        body: &'static str,
    }

    struct LocalHttpServer {
        address: std::net::SocketAddr,
        task: tokio::task::JoinHandle<ObservedRequest>,
    }

    impl LocalHttpServer {
        async fn spawn(response: ResponseSpec) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let task = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                handle_connection(stream, response).await
            });

            Self { address, task }
        }

        fn uri(&self, path: &str) -> Uri {
            format!("http://{}{}", self.address, path).parse().unwrap()
        }

        async fn join(self) -> ObservedRequest {
            self.task.await.unwrap()
        }
    }

    #[derive(Debug)]
    struct ObservedRequest {
        method: String,
        path: String,
        version: String,
        headers: BTreeMap<String, String>,
        body: Bytes,
    }

    async fn handle_connection(mut stream: TcpStream, response: ResponseSpec) -> ObservedRequest {
        let observed = read_request(&mut stream).await;
        let response = format!(
            "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response.status,
            response.body.len(),
            response.body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        observed
    }

    async fn read_request(stream: &mut TcpStream) -> ObservedRequest {
        let mut bytes = Vec::new();
        let header_end = loop {
            let mut buffer = [0; 1024];
            let read = stream.read(&mut buffer).await.unwrap();
            assert_ne!(read, 0, "client closed before request headers arrived");
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(header_end) = find_header_end(&bytes) {
                break header_end;
            }
        };

        let header_text = std::str::from_utf8(&bytes[..header_end]).unwrap();
        let mut lines = header_text.split("\r\n");
        let request_line = lines.next().unwrap();
        let mut request_parts = request_line.split_whitespace();
        let method = request_parts.next().unwrap().to_string();
        let path = request_parts.next().unwrap().to_string();
        let version = request_parts.next().unwrap().to_string();

        let mut headers = BTreeMap::new();
        for line in lines.filter(|line| !line.is_empty()) {
            let (name, value) = line.split_once(':').unwrap();
            headers.insert(name.to_ascii_lowercase(), value.trim().to_string());
        }

        let content_length = headers
            .get("content-length")
            .map(|value| value.parse::<usize>().unwrap())
            .unwrap_or(0);

        let body_start = header_end + 4;
        while bytes.len() < body_start + content_length {
            let mut buffer = [0; 1024];
            let read = stream.read(&mut buffer).await.unwrap();
            assert_ne!(read, 0, "client closed before request body arrived");
            bytes.extend_from_slice(&buffer[..read]);
        }

        ObservedRequest {
            method,
            path,
            version,
            headers,
            body: Bytes::copy_from_slice(&bytes[body_start..body_start + content_length]),
        }
    }

    fn find_header_end(bytes: &[u8]) -> Option<usize> {
        bytes.windows(4).position(|window| window == b"\r\n\r\n")
    }
}
