//! Over-the-air update protocol (pure parts).
//!
//! Flow: after each upload the device calls `GET /device/ota`. The backend only
//! returns a job after the owner approved it in the mobile app. The device streams
//! the image into the inactive slot, verifies size + SHA-256, activates it and
//! reboots. The new image stays pending-verify until its first successful upload,
//! otherwise the bootloader rolls back.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A pending-verify image that cannot upload within this time rolls itself back.
pub const VERIFY_DEADLINE_MS: u64 = 5 * 60_000;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct OtaJob {
    pub job_id: String,
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub status: JobStatus,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Approved,
    Downloading,
    Installing,
    Succeeded,
    Failed,
}

#[derive(Deserialize)]
struct CheckResponse {
    data: Option<OtaJob>,
}

pub fn parse_check_response(body: &[u8]) -> Result<Option<OtaJob>, serde_json::Error> {
    serde_json::from_slice::<CheckResponse>(body).map(|response| response.data)
}

#[derive(Clone, Debug, PartialEq)]
pub enum OtaDecision {
    Nothing,
    Install(OtaJob),
    /// The approved image is already running (fresh boot after install).
    ReportSucceeded(OtaJob),
    /// The job was activated but this (older) image is running: the bootloader rolled back.
    ReportRolledBack(OtaJob),
}

pub fn decide(job: Option<OtaJob>, running_version: &str) -> OtaDecision {
    let Some(job) = job else {
        return OtaDecision::Nothing;
    };
    let running = job.version == running_version;
    match job.status {
        _ if running && job.status != JobStatus::Succeeded && job.status != JobStatus::Failed => {
            OtaDecision::ReportSucceeded(job)
        }
        // Approved, or a download that was interrupted (power loss / error): (re)try.
        JobStatus::Approved | JobStatus::Downloading => OtaDecision::Install(job),
        JobStatus::Installing => OtaDecision::ReportRolledBack(job),
        JobStatus::Succeeded | JobStatus::Failed => OtaDecision::Nothing,
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct StatusReport<'a> {
    pub job_id: &'a str,
    pub status: JobStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub version: &'a str,
}

impl StatusReport<'_> {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Reports download progress at most once per 10 % step.
#[derive(Default)]
pub struct ProgressThrottle {
    last_step: Option<u8>,
}

impl ProgressThrottle {
    pub fn percent(written: u64, total: u64) -> u8 {
        if total == 0 {
            return 0;
        }
        ((written.min(total) * 100) / total) as u8
    }

    pub fn should_report(&mut self, percent: u8) -> bool {
        let step = percent / 10;
        if self.last_step == Some(step) {
            return false;
        }
        self.last_step = Some(step);
        true
    }
}

/// Streaming size + SHA-256 check for a downloaded image.
pub struct ImageVerifier {
    hasher: Sha256,
    written: u64,
    expected_size: u64,
    expected_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VerifyError {
    TooLarge,
    Size { expected: u64, actual: u64 },
    Checksum,
}

impl ImageVerifier {
    pub fn new(job: &OtaJob) -> Self {
        Self {
            hasher: Sha256::new(),
            written: 0,
            expected_size: job.size,
            expected_sha256: job.sha256.trim().to_ascii_lowercase(),
        }
    }

    pub fn update(&mut self, chunk: &[u8]) -> Result<u64, VerifyError> {
        self.written += chunk.len() as u64;
        if self.written > self.expected_size {
            return Err(VerifyError::TooLarge);
        }
        self.hasher.update(chunk);
        Ok(self.written)
    }

    pub fn finish(self) -> Result<(), VerifyError> {
        if self.written != self.expected_size {
            return Err(VerifyError::Size {
                expected: self.expected_size,
                actual: self.written,
            });
        }
        let digest = self.hasher.finalize();
        let actual: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        if actual == self.expected_sha256 {
            Ok(())
        } else {
            Err(VerifyError::Checksum)
        }
    }
}

#[cfg(target_os = "espidf")]
pub mod esp;

#[cfg(test)]
mod tests {
    use super::*;

    fn job(status: JobStatus, version: &str) -> OtaJob {
        OtaJob {
            job_id: "j1".into(),
            version: version.into(),
            url: "https://example.com/fw.bin".into(),
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
            size: 3,
            status,
        }
    }

    #[test]
    fn parses_null_and_job_responses() {
        assert_eq!(parse_check_response(br#"{"data":null}"#).unwrap(), None);
        let body = br#"{"data":{"job_id":"j1","version":"0.2.0","url":"https://x/fw.bin","sha256":"ab","size":10,"status":"approved","extra":1}}"#;
        let parsed = parse_check_response(body).unwrap().unwrap();
        assert_eq!(parsed.version, "0.2.0");
        assert_eq!(parsed.status, JobStatus::Approved);
        assert!(parse_check_response(b"not json").is_err());
    }

    #[test]
    fn decides_install_success_and_rollback() {
        assert_eq!(decide(None, "0.1.0"), OtaDecision::Nothing);
        assert!(matches!(
            decide(Some(job(JobStatus::Approved, "0.2.0")), "0.1.0"),
            OtaDecision::Install(_)
        ));
        assert!(matches!(
            decide(Some(job(JobStatus::Downloading, "0.2.0")), "0.1.0"),
            OtaDecision::Install(_)
        ));
        assert!(matches!(
            decide(Some(job(JobStatus::Installing, "0.2.0")), "0.2.0"),
            OtaDecision::ReportSucceeded(_)
        ));
        assert!(matches!(
            decide(Some(job(JobStatus::Installing, "0.2.0")), "0.1.0"),
            OtaDecision::ReportRolledBack(_)
        ));
        assert!(matches!(
            decide(Some(job(JobStatus::Approved, "0.2.0")), "0.2.0"),
            OtaDecision::ReportSucceeded(_)
        ));
        assert_eq!(
            decide(Some(job(JobStatus::Failed, "0.2.0")), "0.1.0"),
            OtaDecision::Nothing
        );
    }

    #[test]
    fn verifies_size_and_sha256() {
        let mut ok = ImageVerifier::new(&job(JobStatus::Approved, "x"));
        ok.update(b"ab").unwrap();
        ok.update(b"c").unwrap();
        assert_eq!(ok.finish(), Ok(()));

        let mut bad = ImageVerifier::new(&job(JobStatus::Approved, "x"));
        bad.update(b"abd").unwrap();
        assert_eq!(bad.finish(), Err(VerifyError::Checksum));

        let mut short = ImageVerifier::new(&job(JobStatus::Approved, "x"));
        short.update(b"ab").unwrap();
        assert!(matches!(short.finish(), Err(VerifyError::Size { .. })));

        let mut long = ImageVerifier::new(&job(JobStatus::Approved, "x"));
        assert_eq!(long.update(b"abcd"), Err(VerifyError::TooLarge));
    }

    #[test]
    fn throttles_progress_to_ten_percent_steps() {
        let mut throttle = ProgressThrottle::default();
        assert!(throttle.should_report(0));
        assert!(!throttle.should_report(5));
        assert!(throttle.should_report(10));
        assert!(!throttle.should_report(19));
        assert!(throttle.should_report(100));
        assert_eq!(ProgressThrottle::percent(50, 200), 25);
        assert_eq!(ProgressThrottle::percent(300, 200), 100);
        assert_eq!(ProgressThrottle::percent(1, 0), 0);
    }

    #[test]
    fn status_report_json_omits_empty_fields() {
        let json = StatusReport {
            job_id: "j1",
            status: JobStatus::Downloading,
            progress: Some(40),
            error: None,
            version: "0.1.0",
        }
        .to_json();
        assert_eq!(
            json,
            r#"{"job_id":"j1","status":"downloading","progress":40,"version":"0.1.0"}"#
        );
    }
}
