//! How a torrent looks in the downloads list: its state, progress detail and the torrent extras
//! (peers, speeds, ratio), worked out from what the engine reports. No engine types here, so the
//! rules can be tested on their own.

use serde::Serialize;

/// Where a torrent is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    /// Getting the file list from peers (magnet links) or reading the .torrent file.
    Metadata,
    /// Waiting for the user to pick the files to download.
    Choosing,
    /// Checking data already on disk.
    Checking,
    Downloading,
    Paused,
    /// Finished and uploading to others until the ratio limit.
    Seeding,
    /// Finished, no longer uploading.
    Done,
    Error,
}

/// The engine's report for one torrent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sample {
    pub initializing: bool,
    pub paused: bool,
    pub error: Option<String>,
    pub finished: bool,
    /// Bytes downloaded and verified (selected files only).
    pub have: u64,
    /// Bytes of the selected files.
    pub total: u64,
    pub down_bps: u64,
    pub up_bps: u64,
    pub eta_secs: Option<u64>,
    pub peers: u32,
    pub seeders: u32,
}

/// The torrent extras sent with a download event.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentView {
    pub stage: Option<Stage>,
    pub peers: u32,
    pub seeders: u32,
    pub down_speed: u64,
    pub up_speed: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta: Option<u64>,
    pub uploaded: u64,
    pub ratio: f64,
    /// Ratio at which seeding stops (0: no seeding).
    pub ratio_limit: f64,
    pub files: u32,
    pub selected: u32,
    /// Files can be streamed (a single-file torrent or one with selected files).
    pub streamable: bool,
}

/// Uploaded bytes per downloaded byte.
pub fn ratio(uploaded: u64, downloaded: u64) -> f64 {
    if downloaded == 0 {
        return 0.0;
    }
    ((uploaded as f64 / downloaded as f64) * 100.0).round() / 100.0
}

/// The stage of a torrent the engine knows. `seeding_stopped` once the ratio limit was reached or
/// the user stopped seeding.
pub fn stage_of(s: &Sample, seeding_stopped: bool) -> Stage {
    if s.error.is_some() {
        Stage::Error
    } else if s.initializing && !s.paused {
        Stage::Checking
    } else if s.finished {
        if seeding_stopped || s.paused {
            Stage::Done
        } else {
            Stage::Seeding
        }
    } else if s.paused {
        Stage::Paused
    } else {
        Stage::Downloading
    }
}

/// True when a finished torrent has uploaded enough and should stop seeding.
pub fn seeding_done(finished: bool, uploaded: u64, have: u64, limit: f64) -> bool {
    // Exact, not the rounded ratio shown (0.999 has not reached 1.0).
    finished && (limit <= 0.0 || uploaded as f64 >= limit * have as f64)
}

/// The downloads list state for a stage: "progress", "completed" or "interrupted".
pub fn list_state(stage: Stage) -> &'static str {
    match stage {
        Stage::Seeding | Stage::Done => "completed",
        Stage::Error => "interrupted",
        _ => "progress",
    }
}

/// The progress line's detail text for stages the numbers don't explain.
pub fn detail(stage: Stage) -> &'static str {
    match stage {
        Stage::Metadata => "Getting torrent info from peers…",
        Stage::Choosing => "Choose the files to download",
        Stage::Checking => "Checking files…",
        _ => "",
    }
}

/// Keeps a running upload total across the engine's per-session counters, which restart from zero
/// whenever a torrent is paused, resumed or Zenith restarts.
#[derive(Debug, Default, Clone, Copy)]
pub struct UploadCounter {
    last: u64,
}

impl UploadCounter {
    /// Adds what was uploaded since the last reading to `total`.
    pub fn add(&mut self, reading: u64, total: &mut u64) {
        let delta = if reading >= self.last { reading - self.last } else { reading };
        *total = total.saturating_add(delta);
        self.last = reading;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Sample {
        Sample { total: 1000, ..Default::default() }
    }

    #[test]
    fn stages() {
        let s = sample();
        assert_eq!(stage_of(&s, false), Stage::Downloading);
        assert_eq!(stage_of(&Sample { paused: true, ..s.clone() }, false), Stage::Paused);
        assert_eq!(stage_of(&Sample { initializing: true, ..s.clone() }, false), Stage::Checking);
        assert_eq!(stage_of(&Sample { initializing: true, paused: true, ..s.clone() }, false), Stage::Paused);
        assert_eq!(stage_of(&Sample { finished: true, ..s.clone() }, false), Stage::Seeding);
        assert_eq!(stage_of(&Sample { finished: true, ..s.clone() }, true), Stage::Done);
        assert_eq!(stage_of(&Sample { finished: true, paused: true, ..s.clone() }, false), Stage::Done);
        assert_eq!(stage_of(&Sample { error: Some("x".into()), finished: true, ..s }, false), Stage::Error);
        assert_eq!(list_state(Stage::Seeding), "completed");
        assert_eq!(list_state(Stage::Metadata), "progress");
        assert_eq!(list_state(Stage::Error), "interrupted");
    }

    #[test]
    fn ratios() {
        assert_eq!(ratio(0, 0), 0.0);
        assert_eq!(ratio(500, 1000), 0.5);
        assert_eq!(ratio(1, 3), 0.33);
        assert!(!seeding_done(false, 5000, 1000, 1.0));
        assert!(!seeding_done(true, 999, 1000, 1.0));
        assert!(seeding_done(true, 1000, 1000, 1.0));
        assert!(seeding_done(true, 0, 1000, 0.0));
    }

    #[test]
    fn upload_counter_survives_restarts() {
        let (mut c, mut total) = (UploadCounter::default(), 100u64);
        c.add(10, &mut total);
        c.add(25, &mut total);
        assert_eq!(total, 125);
        // Paused and resumed: the engine's counter starts again.
        c.add(0, &mut total);
        c.add(7, &mut total);
        assert_eq!(total, 132);
        // Restarted, and already uploaded a little before this reading.
        c.add(3, &mut total);
        assert_eq!(total, 135);
    }
}
