use crate::models::InstallEvent;
use anyhow::Result;
use reqwest::blocking::Client;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;

/// Generic download with progress events
pub fn download_url<F>(client: &Client, url: &str, on_event: &mut F) -> Result<Vec<u8>>
where
    F: FnMut(InstallEvent),
{
    // Testing
    if url.starts_with("file://") {
        let path = url.trim_start_matches("file://");
        let metadata = fs::metadata(path)?;
        let total_size = metadata.len();

        on_event(InstallEvent::Downloading {
            total_bytes: total_size,
        });
        on_event(InstallEvent::Progress {
            bytes: 0,
            total: total_size,
        });

        let content = fs::read(path)?;

        on_event(InstallEvent::Progress {
            bytes: total_size,
            total: total_size,
        });

        return Ok(content);
    }

    let mut response = client.get(url).send()?.error_for_status()?;
    let total_size = response.content_length().unwrap_or(0);

    on_event(InstallEvent::Downloading {
        total_bytes: total_size,
    });
    on_event(InstallEvent::Progress {
        bytes: 0,
        total: total_size,
    });

    let mut content = Vec::with_capacity(total_size as usize);
    let mut buffer = [0; 8192];

    loop {
        let bytes_read = response.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        content.extend_from_slice(&buffer[..bytes_read]);
        on_event(InstallEvent::Progress {
            bytes: bytes_read as u64,
            total: total_size,
        });
    }

    Ok(content)
}

/// Verify checksum of given content against expected hash
pub fn verify_checksum(content: &[u8], expected_hash: &str) -> Result<()> {
    let mut hasher = Sha256::new();
    hasher.update(content);
    let hash = hex::encode(hasher.finalize());

    if hash != expected_hash {
        anyhow::bail!(
            "Security check failed: Checksum mismatch. Expected: {}, Got: {}",
            expected_hash,
            hash
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_verify_checksum() {
        let data = b"hello world";
        let correct_hash = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        let wrong_hash = "literally-anything-else";

        assert!(verify_checksum(data, correct_hash).is_ok());
        assert!(verify_checksum(data, wrong_hash).is_err());
    }

    #[test]
    fn test_download_url_file_protocol() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let content = b"fake internet content";
        temp_file.write_all(content).unwrap();

        let path = temp_file.path().to_str().unwrap();
        let url = format!("file://{}", path);

        let client = Client::new();
        let mut progress_count = 0;

        let result = download_url(&client, &url, &mut |_| {
            progress_count += 1;
        })
        .unwrap();

        assert_eq!(result, content);
        assert!(
            progress_count > 0,
            "Progress callback should have been called"
        );
    }

    #[test]
    fn test_download_url_file_missing() {
        let client = Client::new();

        let url = "file:///path/to/nowhere/ghost.tar.gz";

        let result = download_url(&client, url, &mut |_| {});

        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("No such file") || err_msg.contains("cannot find"));
    }
}
