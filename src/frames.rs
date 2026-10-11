//! Frame counting, screenshots, and the `GEARBOX_*` environment variables that run an app
//! headless, for a fixed number of frames, or with screenshots, without changing its code.

use std::{path::PathBuf, time::Duration};

use anarchy::Resource;
use anyhow::{Context, bail};
use derive_more::{Deref, DerefMut};
use magician_vgpu::{FrameCapture, glam::UVec2};

/// Resource holding the number of the frame being rendered: 1 during the first frame, and
/// afterwards the number of frames rendered so far.
#[derive(Debug, Default, Deref, DerefMut, Resource, Clone, Copy, PartialEq, Eq)]
pub struct FrameCount(pub u64);

/// Resource of screenshot requests. Each is saved as a PNG when its frame is submitted, see
/// also [`App::with_screenshot`](crate::App::with_screenshot).
#[derive(Debug, Default, Resource)]
pub struct Screenshots {
    /// Paths to save, with the frame to save them at, `None` for the next frame submitted.
    requests: Vec<(Option<u64>, PathBuf)>
}

impl Screenshots {
    /// Save the next frame submitted to `path`. A render system calling this captures the
    /// frame it is drawing.
    pub fn capture(&mut self, path: impl Into<PathBuf>) {
        self.requests.push((None, path.into()));
    }

    /// Save frame `frame` (1 is the first frame) to `path`. A frame that has already been
    /// submitted is captured at the next one instead.
    pub fn capture_at(&mut self, frame: u64, path: impl Into<PathBuf>) {
        self.requests.push((Some(frame), path.into()));
    }

    /// Returns true if no screenshot is waiting to be taken.
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    /// Remove and return the paths due at frame `frame`.
    pub(crate) fn take_due(&mut self, frame: u64) -> Vec<PathBuf> {
        let (due, waiting) = std::mem::take(&mut self.requests).into_iter()
            .partition(|(at, _)| at.is_none_or(|at| at <= frame));
        self.requests = waiting;
        due.into_iter().map(|(_, path)| path).collect()
    }
}

/// Save `capture` to `path` as a PNG, creating parent directories as needed.
pub(crate) fn save_capture(capture: &FrameCapture, path: &PathBuf) -> anyhow::Result<()> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    image::save_buffer(path, &capture.rgba, capture.width, capture.height, image::ColorType::Rgba8)
        .with_context(|| format!("Failed to save screenshot to {}", path.display()))
}

/// The size a headless app renders at when `GEARBOX_HEADLESS` doesn't give one.
pub const DEFAULT_HEADLESS_SIZE: UVec2 = UVec2::new(800, 600);

/// The time step a headless app uses unless one is set: 60 updates and frames per second.
pub const DEFAULT_FIXED_TIMESTEP: Duration = Duration::from_nanos(1_000_000_000 / 60);

/// The frame limit `GEARBOX_SCREENSHOT` implies when `GEARBOX_FRAMES` isn't set.
pub const DEFAULT_SCREENSHOT_FRAME: u64 = 60;

/// Run options read from `GEARBOX_*` environment variables, each `None` when unset.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct EnvOptions {
    /// `GEARBOX_HEADLESS`: `WIDTHxHEIGHT`, or any other value for [`DEFAULT_HEADLESS_SIZE`].
    pub(crate) headless: Option<UVec2>,
    /// `GEARBOX_FRAMES`: exit after this many frames.
    pub(crate) frames: Option<u64>,
    /// `GEARBOX_SCREENSHOT`: save the last frame to this path.
    pub(crate) screenshot: Option<PathBuf>,
    /// `GEARBOX_FIXED_TIMESTEP`: the fixed time step, in seconds.
    pub(crate) fixed_timestep: Option<Duration>,
}

impl EnvOptions {
    /// Read the options from the process environment.
    ///
    /// # Errors
    ///
    /// Fails if a variable is set to a value that can't be parsed.
    pub(crate) fn from_env() -> anyhow::Result<Self> {
        Self::parse(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
    }

    /// Read the options through `var`, which returns a variable's value if it is set.
    pub(crate) fn parse(var: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        Ok(Self {
            headless: var("GEARBOX_HEADLESS").map(|value| parse_size(&value)).transpose()?,
            frames: var("GEARBOX_FRAMES")
                .map(|value| value.parse::<u64>().with_context(|| format!("GEARBOX_FRAMES must be a frame count, not {value:?}")))
                .transpose()?,
            screenshot: var("GEARBOX_SCREENSHOT").map(PathBuf::from),
            fixed_timestep: var("GEARBOX_FIXED_TIMESTEP")
                .map(|value| {
                    let seconds = value.parse::<f64>().ok().filter(|seconds| *seconds > 0.0)
                        .with_context(|| format!("GEARBOX_FIXED_TIMESTEP must be a positive number of seconds, not {value:?}"))?;
                    anyhow::Ok(Duration::from_secs_f64(seconds))
                })
                .transpose()?,
        })
    }
}

/// Parse a `GEARBOX_HEADLESS` value: `WIDTHxHEIGHT`, or anything without an `x` (such as
/// `1`) for [`DEFAULT_HEADLESS_SIZE`].
fn parse_size(value: &str) -> anyhow::Result<UVec2> {
    let Some((width, height)) = value.split_once(['x', 'X']) else { return Ok(DEFAULT_HEADLESS_SIZE) };
    match (width.trim().parse::<u32>(), height.trim().parse::<u32>()) {
        (Ok(width), Ok(height)) if width > 0 && height > 0 => Ok(UVec2::new(width, height)),
        _ => bail!("GEARBOX_HEADLESS must be WIDTHxHEIGHT (like 800x600) or 1, not {value:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let vars: Vec<(String, String)> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |name| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
    }

    #[test]
    fn unset_variables_parse_to_none() {
        assert_eq!(EnvOptions::parse(env(&[])).unwrap(), EnvOptions::default());
    }

    #[test]
    fn variables_parse() {
        let options = EnvOptions::parse(env(&[
            ("GEARBOX_HEADLESS", "640x480"),
            ("GEARBOX_FRAMES", "10"),
            ("GEARBOX_SCREENSHOT", "out/shot.png"),
            ("GEARBOX_FIXED_TIMESTEP", "0.5"),
        ])).unwrap();
        assert_eq!(options.headless, Some(UVec2::new(640, 480)));
        assert_eq!(options.frames, Some(10));
        assert_eq!(options.screenshot, Some(PathBuf::from("out/shot.png")));
        assert_eq!(options.fixed_timestep, Some(Duration::from_millis(500)));
    }

    #[test]
    fn headless_without_a_size_uses_the_default() {
        assert_eq!(parse_size("1").unwrap(), DEFAULT_HEADLESS_SIZE);
        assert_eq!(parse_size("true").unwrap(), DEFAULT_HEADLESS_SIZE);
    }

    #[test]
    fn bad_values_are_errors() {
        assert!(parse_size("0x10").is_err());
        assert!(parse_size("wide x tall").is_err());
        assert!(EnvOptions::parse(env(&[("GEARBOX_FRAMES", "many")])).is_err());
        assert!(EnvOptions::parse(env(&[("GEARBOX_FIXED_TIMESTEP", "-1")])).is_err());
    }

    #[test]
    fn screenshots_are_taken_when_due() {
        let mut screenshots = Screenshots::default();
        screenshots.capture_at(3, "three.png");
        screenshots.capture("next.png");
        assert_eq!(screenshots.take_due(1), vec![PathBuf::from("next.png")]);
        assert!(screenshots.take_due(2).is_empty());
        assert_eq!(screenshots.take_due(5), vec![PathBuf::from("three.png")]);
        assert!(screenshots.is_empty());
    }
}
