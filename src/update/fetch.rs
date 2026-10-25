//! Talking to GitHub: the release check and the download-and-install job.
//!
//! Each job runs on its own thread with a current-thread tokio runtime and
//! reports once over an `mpsc` channel, like `openapi::source::fetch_url`,
//! so the UI thread only ever polls.

use std::future::Future;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use super::release::{self, Asset, Release};
use super::{UpdateError, VERSION, install};

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/neisep/Probe/releases/latest";
const CHECK_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
/// Refuse anything larger; a Probe binary is a few tens of MB.
const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug)]
pub enum CheckOutcome {
    UpToDate,
    Available(Box<Release>),
}

/// Ask GitHub for the latest release and decide whether this build should be
/// offered it.
pub fn spawn_check() -> mpsc::Receiver<Result<CheckOutcome, UpdateError>> {
    spawn(CHECK_TIMEOUT, |client| async move {
        let response = client
            .get(LATEST_RELEASE_URL)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .map_err(http_error)?;
        // GitHub answers 404 while the repository has no published release.
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(CheckOutcome::UpToDate);
        }
        if !response.status().is_success() {
            return Err(UpdateError::Http(format!("HTTP {}", response.status())));
        }
        let release: Release = response.json().await.map_err(http_error)?;
        let offer = release::worth_offering(&release, VERSION, release::this_build_asset_name());
        Ok(if offer {
            CheckOutcome::Available(Box::new(release))
        } else {
            CheckOutcome::UpToDate
        })
    })
}

/// Download this build's asset from `release`, verify it against the
/// release's `SHA256SUMS`, and replace `exe` with it.
pub fn spawn_install(release: Release, exe: PathBuf) -> mpsc::Receiver<Result<(), UpdateError>> {
    spawn(DOWNLOAD_TIMEOUT, move |client| async move {
        let asset_name = release::this_build_asset_name().ok_or_else(|| {
            UpdateError::Install("no release binary is built for this platform".to_owned())
        })?;
        let missing = |name: &str| UpdateError::Install(format!("release has no {name}"));
        let asset = release
            .asset(asset_name)
            .ok_or_else(|| missing(asset_name))?;
        let sums_asset = release
            .asset(release::SUMS_ASSET)
            .ok_or_else(|| missing(release::SUMS_ASSET))?;

        // Fail on an unwritable install directory before downloading anything.
        let staged = install::stage_next_to(&exe)?;

        let sums = download(&client, sums_asset).await?;
        let sums = String::from_utf8(sums)
            .map_err(|_| UpdateError::Integrity("SHA256SUMS is not text".to_owned()))?;
        let expected = release::expected_sha256(&sums, asset_name).ok_or_else(|| {
            UpdateError::Integrity(format!("SHA256SUMS has no entry for {asset_name}"))
        })?;

        let bytes = download(&client, asset).await?;
        install::install(&exe, staged, &bytes, &expected)
    })
}

/// Fetch an asset whose size GitHub already told us, refusing more bytes
/// than that (or than the cap) and fewer.
async fn download(client: &reqwest::Client, asset: &Asset) -> Result<Vec<u8>, UpdateError> {
    if asset.size > MAX_DOWNLOAD_BYTES {
        return Err(UpdateError::Integrity(format!(
            "{} is {} bytes, over the {MAX_DOWNLOAD_BYTES}-byte limit",
            asset.name, asset.size
        )));
    }
    let mut response = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .map_err(http_error)?;
    if !response.status().is_success() {
        return Err(UpdateError::Http(format!(
            "downloading {}: HTTP {}",
            asset.name,
            response.status()
        )));
    }

    let capacity = usize::try_from(asset.size).unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);
    while let Some(chunk) = response.chunk().await.map_err(http_error)? {
        if bytes.len() as u64 + chunk.len() as u64 > asset.size {
            return Err(UpdateError::Integrity(format!(
                "{} is larger than the release says",
                asset.name
            )));
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() as u64 != asset.size {
        return Err(UpdateError::Integrity(format!(
            "{} download incomplete ({} of {} bytes)",
            asset.name,
            bytes.len(),
            asset.size
        )));
    }
    Ok(bytes)
}

fn http_error(error: reqwest::Error) -> UpdateError {
    UpdateError::Http(error.to_string())
}

fn spawn<T, F, Fut>(timeout: Duration, job: F) -> mpsc::Receiver<Result<T, UpdateError>>
where
    T: Send + 'static,
    F: FnOnce(reqwest::Client) -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, UpdateError>>,
{
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| UpdateError::Http(format!("runtime: {error}")))
            .and_then(|runtime| {
                let client = reqwest::Client::builder()
                    .user_agent(format!("probe/{VERSION}"))
                    .https_only(true)
                    .connect_timeout(Duration::from_secs(10))
                    .timeout(timeout)
                    .build()
                    .map_err(http_error)?;
                runtime.block_on(job(client))
            });
        let _ = tx.send(result);
    });
    rx
}
