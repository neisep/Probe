use crate::runtime::types::*;
use reqwest::Method;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc as std_mpsc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, mpsc};

/// How long `Runtime::new` will wait for the worker thread to confirm
/// it has built the tokio runtime + reqwest client. The worker is
/// expected to be ready in sub-millisecond time on every supported
/// platform, so this is purely a defensive ceiling that prevents
/// hangs if something genuinely catastrophic happens.
const WORKER_READY_TIMEOUT: Duration = Duration::from_secs(3);

/// Connection establishment timeout. Tight enough that DNS / TLS hangs surface
/// quickly while staying generous for slow tunnels.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// End-to-end request timeout including body read. Slow APIs that legitimately
/// take longer can be served by editing this constant; we'd rather show users
/// a clear timeout error than pin a worker forever.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Hard cap on the response body buffered into memory. Anything beyond this is
/// dropped on the floor and `ResponseInfo.truncated` is set so the UI can warn.
const MAX_RESPONSE_BYTES: usize = 100 * 1024 * 1024;

/// Small internal work item.
struct WorkItem {
    id: RequestId,
    req: AsyncRequest,
}

struct RuntimeInner {
    tx: mpsc::Sender<WorkItem>,
    // Shared state for status/results/events
    state: Mutex<SharedState>,
    id_counter: AtomicU64,
}

struct SharedState {
    statuses: HashMap<RequestId, RequestStatus>,
    results: HashMap<RequestId, AsyncRequestResult>,
    events: VecDeque<Event>,
}

/// Runtime handle - cloneable and cheap.
#[derive(Clone)]
pub struct Runtime {
    inner: Arc<RuntimeInner>,
}

impl Runtime {
    /// Create a new runtime with an internal submission buffer.
    /// buffer_size controls the mpsc channel capacity for pending requests.
    ///
    /// Returns an error if the background worker thread cannot get a
    /// tokio runtime + reqwest client up and running within
    /// `WORKER_READY_TIMEOUT`. Surfacing the failure here means callers
    /// see a real error message (and the UI status shows "Runtime
    /// unavailable: …") instead of the previous behaviour where the
    /// worker silently exited and every submitted request hung in
    /// `Pending` forever.
    pub fn new(buffer_size: usize) -> Result<Self, String> {
        let (tx, mut rx) = mpsc::channel::<WorkItem>(buffer_size);
        let inner = Arc::new(RuntimeInner {
            tx,
            state: Mutex::new(SharedState {
                statuses: HashMap::new(),
                results: HashMap::new(),
                events: VecDeque::new(),
            }),
            id_counter: AtomicU64::new(1),
        });

        // Clone for worker
        let worker_inner = inner.clone();

        // Readiness handshake — the worker reports Ok(()) once both
        // the tokio runtime and the reqwest client are built, or an
        // error string explaining which step failed.
        let (ready_tx, ready_rx) = std_mpsc::sync_channel::<Result<(), String>>(1);

        // Spawn a dedicated background thread that runs a Tokio runtime to drive submissions.
        // This keeps the UI/main thread free of Tokio runtime requirements.
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = ready_tx.send(Err(format!("tokio runtime: {e}")));
                    return;
                }
            };

            rt.block_on(async move {
                let client = match reqwest::Client::builder()
                    .connect_timeout(CONNECT_TIMEOUT)
                    .timeout(REQUEST_TIMEOUT)
                    .pool_idle_timeout(Duration::from_secs(30))
                    .build()
                {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = ready_tx.send(Err(format!("reqwest client: {e}")));
                        return;
                    }
                };

                // Worker is fully initialised — release the constructor.
                let _ = ready_tx.send(Ok(()));

                while let Some(item) = rx.recv().await {
                    let client = client.clone();
                    let inner = worker_inner.clone();
                    tokio::spawn(async move {
                        // mark in-progress
                        {
                            let mut st = inner.state.lock().await;
                            st.statuses.insert(item.id, RequestStatus::InProgress);
                            st.events.push_back(Event::StatusChanged {
                                id: item.id,
                                status: RequestStatus::InProgress,
                            });
                        }

                        // perform the request
                        let result = do_request(&client, &item.req).await;

                        // store result and emit event
                        {
                            let mut st = inner.state.lock().await;
                            match result {
                                Ok(resp) => {
                                    st.results
                                        .insert(item.id, AsyncRequestResult::Ok(resp.clone()));
                                    st.statuses.insert(item.id, RequestStatus::Completed);
                                    st.events.push_back(Event::Completed {
                                        id: item.id,
                                        result: AsyncRequestResult::Ok(resp),
                                    });
                                }
                                Err(err) => {
                                    st.results
                                        .insert(item.id, AsyncRequestResult::Err(err.clone()));
                                    st.statuses.insert(item.id, RequestStatus::Failed);
                                    st.events.push_back(Event::Completed {
                                        id: item.id,
                                        result: AsyncRequestResult::Err(err),
                                    });
                                }
                            }
                        }
                    });
                }
            });
        });

        // Block briefly for the worker to become ready. If the runtime
        // or client failed to build, the worker has already exited and
        // we propagate the underlying cause; if the channel disconnects
        // without a message, the worker panicked before reporting.
        match ready_rx.recv_timeout(WORKER_READY_TIMEOUT) {
            Ok(Ok(())) => Ok(Self { inner }),
            Ok(Err(message)) => Err(format!("worker failed to start: {message}")),
            Err(std_mpsc::RecvTimeoutError::Timeout) => Err(format!(
                "worker did not become ready within {:?}",
                WORKER_READY_TIMEOUT
            )),
            Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                Err("worker exited before signalling ready".to_owned())
            }
        }
    }

    /// Submit a request. Returns the assigned RequestId or a string error.
    #[allow(dead_code)]
    pub async fn submit(&self, req: AsyncRequest) -> Result<RequestId, String> {
        let id = self.inner.id_counter.fetch_add(1, Ordering::Relaxed);
        // Register pending. The resolved request itself (including any
        // bearer/api-key headers) is forwarded to the worker but NOT
        // retained in SharedState — keeping it would leave a live copy
        // of every credential in memory for the session lifetime.
        {
            let mut st = self.inner.state.lock().await;
            st.statuses.insert(id, RequestStatus::Pending);
            st.events.push_back(Event::StatusChanged {
                id,
                status: RequestStatus::Pending,
            });
        }

        let item = WorkItem { id, req };
        self.inner
            .tx
            .send(item)
            .await
            .map_err(|e| format!("submit failed: {}", e))?;
        Ok(id)
    }

    #[allow(dead_code)]
    pub async fn submit_resolved(
        &self,
        req: AsyncRequest,
        values: &ResolutionValues,
        behavior: UnresolvedBehavior,
    ) -> Result<RequestId, ErrorInfo> {
        let prepared = prepare_request(&req, values, behavior).map_err(|e| e.to_error_info())?;
        self.submit(prepared).await.map_err(submit_error)
    }

    /// Poll (and drain) pending events. Designed for UI-safe polling.
    #[allow(dead_code)]
    pub async fn poll_events(&self) -> Vec<Event> {
        let mut st = self.inner.state.lock().await;
        let mut out = Vec::new();
        while let Some(ev) = st.events.pop_front() {
            out.push(ev);
        }
        out
    }

    /// Synchronous/blocking submit helper for UI threads that are not async.
    /// Uses blocking variants of the internal synchronization primitives.
    pub fn submit_blocking(&self, req: AsyncRequest) -> Result<RequestId, String> {
        let id = self.inner.id_counter.fetch_add(1, Ordering::Relaxed);
        // register pending (blocking) — see `submit()` for why we don't
        // retain the resolved request in SharedState.
        {
            let mut st = self.inner.state.blocking_lock();
            st.statuses.insert(id, RequestStatus::Pending);
            st.events.push_back(Event::StatusChanged {
                id,
                status: RequestStatus::Pending,
            });
        }

        let item = WorkItem { id, req };
        self.inner
            .tx
            .blocking_send(item)
            .map_err(|e| format!("submit failed: {}", e))?;
        Ok(id)
    }

    #[allow(dead_code)]
    pub fn submit_resolved_blocking(
        &self,
        req: AsyncRequest,
        values: &ResolutionValues,
        behavior: UnresolvedBehavior,
    ) -> Result<RequestId, ErrorInfo> {
        let prepared = prepare_request(&req, values, behavior).map_err(|e| e.to_error_info())?;
        self.submit_blocking(prepared).map_err(submit_error)
    }

    /// Synchronous/blocking poll of runtime events. Drains available events.
    pub fn poll_events_blocking(&self) -> Vec<Event> {
        let mut st = self.inner.state.blocking_lock();
        let mut out = Vec::new();
        while let Some(ev) = st.events.pop_front() {
            out.push(ev);
        }
        out
    }

    /// Query status for a given request id.
    #[allow(dead_code)]
    pub async fn get_status(&self, id: RequestId) -> Option<RequestStatus> {
        let st = self.inner.state.lock().await;
        st.statuses.get(&id).cloned()
    }

    /// Try to cancel a pending request. This is best-effort: if a request has moved
    /// to InProgress it cannot be cancelled here. Returns true if cancellation succeeded.
    #[allow(dead_code)]
    pub async fn cancel(&self, id: RequestId) -> bool {
        let mut st = self.inner.state.lock().await;
        match st.statuses.get(&id).cloned() {
            Some(RequestStatus::Pending) => {
                st.statuses.insert(id, RequestStatus::Cancelled);
                st.events.push_back(Event::StatusChanged {
                    id,
                    status: RequestStatus::Cancelled,
                });
                // No running task was started yet (it will be ignored by the worker when picked up),
                // we also insert a result placeholder.
                st.results.insert(
                    id,
                    AsyncRequestResult::Err(ErrorInfo::new(
                        "cancelled".to_string(),
                        None,
                        None,
                        Some("cancelled".to_string()),
                    )),
                );
                true
            }
            _ => false,
        }
    }

    /// Try to retrieve a result if available.
    #[allow(dead_code)]
    pub async fn take_result(&self, id: RequestId) -> Option<AsyncRequestResult> {
        let mut st = self.inner.state.lock().await;
        st.results.remove(&id)
    }
}

async fn do_request(client: &reqwest::Client, r: &AsyncRequest) -> Result<ResponseInfo, ErrorInfo> {
    do_request_with_limit(client, r, MAX_RESPONSE_BYTES).await
}

async fn do_request_with_limit(
    client: &reqwest::Client,
    r: &AsyncRequest,
    max_body_bytes: usize,
) -> Result<ResponseInfo, ErrorInfo> {
    let method = parse_method(&r.method)?;
    let builder = apply_request_headers(client.request(method.clone(), &r.url), &r.headers)?;
    let builder = apply_request_body(builder, &method, r.body.as_deref());
    let start = Instant::now();
    let resp_res = builder.send().await;
    let duration = start.elapsed().as_millis();

    match resp_res {
        Ok(resp) => {
            let status = resp.status().as_u16();
            // collect headers and extract a content-type hint when available
            let mut headers_out = Vec::new();
            let mut content_type: Option<String> = None;
            for (k, v) in resp.headers().iter() {
                let name = k.as_str().to_string();
                let value = match v.to_str() {
                    Ok(s) => s.to_string(),
                    Err(_) => format!("<binary:{:?}>", v.as_bytes()),
                };
                if name.eq_ignore_ascii_case("content-type") {
                    content_type = Some(value.clone());
                }
                headers_out.push((name, value));
            }

            match read_body_capped(resp, max_body_bytes).await {
                Ok((body, truncated)) => Ok(ResponseInfo {
                    status,
                    body,
                    headers: headers_out,
                    content_type,
                    duration_ms: duration,
                    truncated,
                }),
                Err(e) => Err(ErrorInfo::new(
                    "reading body failed".to_string(),
                    Some(status),
                    Some(e.to_string()),
                    Some("body-read".to_string()),
                )),
            }
        }
        Err(e) => Err(ErrorInfo::new(
            send_error_message(&e).to_string(),
            None,
            Some(e.to_string()),
            Some(classify_send_error(&e).to_string()),
        )),
    }
}

/// Drain a `reqwest::Response` into a `Vec<u8>` bounded by `max_bytes`.
///
/// Returns `(body, truncated)`. When the server sends more than `max_bytes`,
/// we keep the first `max_bytes` and discard the rest — the connection is
/// dropped on return, so this is also a soft cancellation of the transfer.
async fn read_body_capped(
    mut resp: reqwest::Response,
    max_bytes: usize,
) -> Result<(Vec<u8>, bool), reqwest::Error> {
    let mut buf: Vec<u8> = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = resp.chunk().await? {
        if buf.len() >= max_bytes {
            truncated = true;
            break;
        }
        let remaining = max_bytes - buf.len();
        if chunk.len() > remaining {
            buf.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }
        buf.extend_from_slice(&chunk);
    }
    Ok((buf, truncated))
}

fn classify_send_error(err: &reqwest::Error) -> &'static str {
    if err.is_timeout() {
        "timeout"
    } else if err.is_connect() {
        "connect"
    } else if err.is_redirect() {
        "redirect"
    } else {
        "request"
    }
}

fn send_error_message(err: &reqwest::Error) -> &'static str {
    if err.is_timeout() {
        "request timed out"
    } else if err.is_connect() {
        "connection failed"
    } else {
        "request failed"
    }
}

fn parse_method(raw: &str) -> Result<Method, ErrorInfo> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "GET" => Ok(Method::GET),
        "POST" => Ok(Method::POST),
        "PUT" => Ok(Method::PUT),
        "PATCH" => Ok(Method::PATCH),
        "DELETE" => Ok(Method::DELETE),
        "OPTIONS" => Ok(Method::OPTIONS),
        "HEAD" => Ok(Method::HEAD),
        other => Err(ErrorInfo::new(
            format!("invalid method: {other}"),
            None,
            None,
            Some("invalid-method".to_string()),
        )),
    }
}

fn apply_request_body(
    builder: reqwest::RequestBuilder,
    method: &Method,
    body: Option<&[u8]>,
) -> reqwest::RequestBuilder {
    match body {
        Some(body) if method_supports_body(method) => builder.body(body.to_vec()),
        _ => builder,
    }
}

fn method_supports_body(method: &Method) -> bool {
    matches!(
        method.as_str(),
        "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
    )
}

fn apply_request_headers(
    builder: reqwest::RequestBuilder,
    headers: &RequestHeaders,
) -> Result<reqwest::RequestBuilder, ErrorInfo> {
    let mut header_map = HeaderMap::with_capacity(headers.len());

    for (name, value) in headers {
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() && value.trim().is_empty() {
            continue;
        }
        if trimmed_name.is_empty() {
            return Err(ErrorInfo::new(
                "invalid request header".to_string(),
                None,
                Some("header name is empty".to_string()),
                Some("invalid-header".to_string()),
            ));
        }

        let header_name = match HeaderName::from_bytes(trimmed_name.as_bytes()) {
            Ok(header_name) => header_name,
            Err(e) => {
                return Err(ErrorInfo::new(
                    "invalid request header".to_string(),
                    None,
                    Some(format!("invalid header name `{trimmed_name}`: {e}")),
                    Some("invalid-header".to_string()),
                ));
            }
        };
        let header_value = match HeaderValue::from_str(value) {
            Ok(header_value) => header_value,
            Err(e) => {
                return Err(ErrorInfo::new(
                    "invalid request header".to_string(),
                    None,
                    Some(format!("invalid header value for `{trimmed_name}`: {e}")),
                    Some("invalid-header".to_string()),
                ));
            }
        };

        header_map.append(header_name, header_value);
    }

    Ok(builder.headers(header_map))
}

#[allow(dead_code)]
pub fn prepare_request(
    req: &AsyncRequest,
    values: &ResolutionValues,
    behavior: UnresolvedBehavior,
) -> Result<AsyncRequest, ResolutionError> {
    req.resolve_with_behavior(values, behavior)
}

#[allow(dead_code)]
fn submit_error(details: String) -> ErrorInfo {
    ErrorInfo::new(
        "submit failed".to_string(),
        None,
        Some(details),
        Some("submit".to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Spin up a one-shot HTTP/1.1 server that returns a body of `body_len` bytes.
    /// Returns the bound `http://127.0.0.1:<port>` URL once the listener is live.
    async fn spawn_oneshot_server(body_len: usize) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().expect("local_addr");
        let url = format!("http://{addr}/");

        tokio::spawn(async move {
            let (mut socket, _) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => return,
            };
            // Drain the request headers — we don't care about the contents,
            // we just need to read up to the blank line so the client knows
            // we're ready to write the response.
            let mut scratch = [0u8; 1024];
            // Best-effort single read; for a simple GET this captures the
            // entire request preamble.
            let _ = socket.read(&mut scratch).await;

            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {body_len}\r\n\r\n"
            );
            if socket.write_all(header.as_bytes()).await.is_err() {
                return;
            }
            // Write the body in fixed-size chunks so the client's chunked
            // reader actually has multiple chunks to iterate over.
            let chunk = vec![b'x'; 4096];
            let mut written = 0;
            while written < body_len {
                let take = std::cmp::min(chunk.len(), body_len - written);
                if socket.write_all(&chunk[..take]).await.is_err() {
                    return;
                }
                written += take;
            }
            let _ = socket.shutdown().await;
        });

        url
    }

    fn test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(5))
            .build()
            .expect("build test client")
    }

    #[tokio::test]
    async fn body_under_cap_is_not_truncated() {
        let url = spawn_oneshot_server(1024).await;
        let req = AsyncRequest {
            url,
            method: "GET".into(),
            headers: Vec::new(),
            body: None,
        };
        let client = test_client();
        let info = do_request_with_limit(&client, &req, 64 * 1024)
            .await
            .expect("request succeeded");
        assert_eq!(info.status, 200);
        assert_eq!(info.body.len(), 1024);
        assert!(!info.truncated, "small body should not be marked truncated");
    }

    #[tokio::test]
    async fn body_over_cap_is_truncated_to_limit() {
        const CAP: usize = 8 * 1024;
        let url = spawn_oneshot_server(64 * 1024).await;
        let req = AsyncRequest {
            url,
            method: "GET".into(),
            headers: Vec::new(),
            body: None,
        };
        let client = test_client();
        let info = do_request_with_limit(&client, &req, CAP)
            .await
            .expect("request succeeded");
        assert_eq!(info.status, 200);
        assert_eq!(info.body.len(), CAP, "body should be capped at CAP bytes");
        assert!(info.truncated, "oversized body must be marked truncated");
    }

    /// Smoke test for the worker-readiness handshake added in M5:
    /// `Runtime::new` should return Ok within the timeout, and the worker
    /// it spawns must actually be able to drive a real HTTP request
    /// end-to-end. A regression where the handshake reports Ok but the
    /// worker is broken would be caught by the polled completion event.
    #[tokio::test(flavor = "multi_thread")]
    async fn runtime_new_signals_ready_and_processes_a_request() {
        let url = spawn_oneshot_server(64).await;
        let start = Instant::now();
        let runtime = Runtime::new(4).expect("Runtime::new must succeed");
        assert!(
            start.elapsed() < WORKER_READY_TIMEOUT,
            "Runtime::new must return well inside the worker-ready timeout"
        );

        let req = AsyncRequest {
            url,
            method: "GET".into(),
            headers: Vec::new(),
            body: None,
        };
        let _id = runtime.submit(req).await.expect("submit");

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let events = runtime.poll_events().await;
            if events
                .iter()
                .any(|ev| matches!(ev, Event::Completed { .. }))
            {
                return;
            }
            if Instant::now() >= deadline {
                panic!("did not observe a Completed event within 2s");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}
