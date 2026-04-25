use super::OpenApiError;
use std::sync::mpsc;

/// Fetch the text of an OpenAPI spec from a URL.
/// Spawns a background thread and returns a channel receiver — does not block the caller.
pub fn fetch_url(url: &str) -> mpsc::Receiver<Result<String, OpenApiError>> {
    let url = url.to_owned();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| OpenApiError::Http(e.to_string()))
            .and_then(|rt| {
                rt.block_on(async {
                    let response = reqwest::get(&url)
                        .await
                        .map_err(|e| OpenApiError::Http(e.to_string()))?;
                    if !response.status().is_success() {
                        return Err(OpenApiError::Http(format!(
                            "HTTP {}",
                            response.status()
                        )));
                    }
                    response
                        .text()
                        .await
                        .map_err(|e| OpenApiError::Http(e.to_string()))
                })
            });
        let _ = tx.send(result);
    });
    rx
}
