use crate::runtime::types::*;
use reqwest::Method;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::sync::{Mutex, mpsc};

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
    /// Keep a copy of submitted requests so UIs can echo method/url/label
    requests: HashMap<RequestId, AsyncRequest>,
}

/// Runtime handle - cloneable and cheap.
#[derive(Clone)]
pub struct Runtime {
    inner: Arc<RuntimeInner>,
}

impl Runtime {
    /// Create a new runtime with an internal submission buffer.
    /// buffer_size controls the mpsc channel capacity for pending requests.
    pub fn new(buffer_size: usize) -> Result<Self, String> {
        let (tx, mut rx) = mpsc::channel::<WorkItem>(buffer_size);
        let inner = Arc::new(RuntimeInner {
            tx,
            state: Mutex::new(SharedState {
                statuses: HashMap::new(),
                results: HashMap::new(),
                events: VecDeque::new(),
                requests: HashMap::new(),
            }),
            id_counter: AtomicU64::new(1),
        });

        // Clone for worker
        let worker_inner = inner.clone();

        // Spawn a dedicated background thread that runs a Tokio runtime to drive submissions.
        // This keeps the UI/main thread free of Tokio runtime requirements.
        std::thread::spawn(move || {
            let runtime_result = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build();

            let Ok(rt) = runtime_result else {
                return;
            };

            rt.block_on(async move {
                let client = reqwest::Client::new();
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

        Ok(Self { inner })
    }

    /// Submit a request. Returns the assigned RequestId or a string error.
    pub async fn submit(&self, req: AsyncRequest) -> Result<RequestId, String> {
        let id = self.inner.id_counter.fetch_add(1, Ordering::Relaxed);
        // register pending
        {
            let mut st = self.inner.state.lock().await;
            st.statuses.insert(id, RequestStatus::Pending);
            // store request metadata for UI/inspection
            st.requests.insert(id, req.clone());
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

    /// Poll (and drain) pending events. Designed for UI-safe polling.
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
        // register pending (blocking)
        {
            let mut st = self.inner.state.blocking_lock();
            st.statuses.insert(id, RequestStatus::Pending);
            st.requests.insert(id, req.clone());
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
    pub async fn get_status(&self, id: RequestId) -> Option<RequestStatus> {
        let st = self.inner.state.lock().await;
        st.statuses.get(&id).cloned()
    }

    /// Retrieve stored request metadata (method/url/label/headers) if available.
    pub async fn get_request(&self, id: RequestId) -> Option<AsyncRequest> {
        let st = self.inner.state.lock().await;
        st.requests.get(&id).cloned()
    }

    /// Try to cancel a pending request. This is best-effort: if a request has moved
    /// to InProgress it cannot be cancelled here. Returns true if cancellation succeeded.
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
    pub async fn take_result(&self, id: RequestId) -> Option<AsyncRequestResult> {
        let mut st = self.inner.state.lock().await;
        st.results.remove(&id)
    }
}

async fn do_request(client: &reqwest::Client, r: &AsyncRequest) -> Result<ResponseInfo, ErrorInfo> {
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

            match resp.bytes().await {
                Ok(bytes) => Ok(ResponseInfo {
                    status,
                    body: bytes.to_vec(),
                    headers: headers_out,
                    content_type,
                    duration_ms: duration,
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
            "request failed".to_string(),
            None,
            Some(e.to_string()),
            Some("request".to_string()),
        )),
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
