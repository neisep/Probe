use super::OpenApiError;

/// Fetch the text of an OpenAPI spec from a URL.
/// Blocks the calling thread using a one-shot tokio runtime.
pub fn fetch_url(url: &str) -> Result<String, OpenApiError> {
    let url = url.to_owned();
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| OpenApiError::Http(e.to_string()))?
            .block_on(async {
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
    })
    .join()
    .map_err(|payload| {
        let msg = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "fetch thread panicked (unknown payload)".to_owned());
        OpenApiError::Http(msg)
    })?
}
