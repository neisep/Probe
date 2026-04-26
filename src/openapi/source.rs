use super::OpenApiError;
use std::sync::mpsc;

fn validate_scheme(url: &str) -> Result<(), OpenApiError> {
    let parsed = url::Url::parse(url).map_err(|e| OpenApiError::Http(e.to_string()))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        scheme => Err(OpenApiError::Http(format!(
            "URL scheme '{scheme}' is not allowed; only http and https are permitted"
        ))),
    }
}

/// Fetch the text of an OpenAPI spec from a URL.
/// Spawns a background thread and returns a channel receiver — does not block the caller.
pub fn fetch_url(url: &str) -> mpsc::Receiver<Result<String, OpenApiError>> {
    let url = url.to_owned();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = validate_scheme(&url).and_then(|_| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| OpenApiError::Http(e.to_string()))
                .and_then(|rt| {
                    rt.block_on(async {
                        let client = reqwest::Client::builder()
                            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                                let scheme = attempt.url().scheme().to_owned();
                                if scheme == "http" || scheme == "https" {
                                    attempt.follow()
                                } else {
                                    attempt.error(format!(
                                        "redirect to disallowed scheme '{scheme}'"
                                    ))
                                }
                            }))
                            .build()
                            .map_err(|e| OpenApiError::Http(e.to_string()))?;
                        let response = client
                            .get(&url)
                            .send()
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
        });
        let _ = tx.send(result);
    });
    rx
}
