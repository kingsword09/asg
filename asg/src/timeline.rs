use std::time::Duration;

use anyhow::{Result, bail};

use crate::asciicast::{Asciicast, EventKind};
use crate::terminal::{Snapshot, Terminal};

#[derive(Debug, Clone)]
pub struct TimelineOptions {
    pub speed: f64,
    pub fps: u16,
    pub idle_time_limit: Option<f64>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    pub cursor: bool,
    pub at: Option<f64>,
    pub from: Option<f64>,
    pub to: Option<f64>,
}

impl Default for TimelineOptions {
    fn default() -> Self {
        Self {
            speed: 1.0,
            fps: 30,
            idle_time_limit: None,
            cols: None,
            rows: None,
            cursor: true,
            at: None,
            from: None,
            to: None,
        }
    }
}

#[derive(Clone)]
pub struct Frame {
    pub time: f64,
    pub snapshot: Snapshot,
}

pub struct Timeline {
    pub cols: usize,
    pub rows: usize,
    pub duration: f64,
    pub frames: Vec<Frame>,
}

pub fn build(cast: &Asciicast, options: &TimelineOptions) -> Result<Timeline> {
    validate(options)?;

    let initial_cols = usize::from(options.cols.unwrap_or(cast.header.term.cols));
    let initial_rows = usize::from(options.rows.unwrap_or(cast.header.term.rows));
    let mut terminal = Terminal::new(initial_cols, initial_rows);
    let mut canvas_cols = initial_cols;
    let mut canvas_rows = initial_rows;
    let mut frames = vec![Frame {
        time: 0.0,
        snapshot: terminal.snapshot(options.cursor),
    }];

    let idle_limit = options
        .idle_time_limit
        .or(cast.header.idle_time_limit)
        .map(Duration::from_secs_f64);
    let mut source_time = Duration::ZERO;
    let mut output_time = 0.0;

    for event in &cast.events {
        let mut delay = event.time.saturating_sub(source_time);
        source_time = event.time;
        if let Some(limit) = idle_limit {
            delay = delay.min(limit);
        }
        output_time += delay.as_secs_f64() / options.speed;

        let visual = match &event.kind {
            EventKind::Output(data) => {
                terminal.feed(data);
                true
            }
            EventKind::Resize { cols, rows } => {
                let cols = usize::from(options.cols.unwrap_or(*cols));
                let rows = usize::from(options.rows.unwrap_or(*rows));
                terminal.resize(cols, rows);
                canvas_cols = canvas_cols.max(cols);
                canvas_rows = canvas_rows.max(rows);
                true
            }
            EventKind::Input(_)
            | EventKind::Marker(_)
            | EventKind::Exit(_)
            | EventKind::Other { .. } => false,
        };

        if visual {
            push_visual_frame(
                &mut frames,
                Frame {
                    time: output_time,
                    snapshot: terminal.snapshot(options.cursor),
                },
            );
        }
    }

    let mut timeline = select(frames, canvas_cols, canvas_rows, output_time, options)?;
    timeline.frames = cap_fps(timeline.frames, options.fps);

    Ok(timeline)
}

fn validate(options: &TimelineOptions) -> Result<()> {
    if !options.speed.is_finite() || options.speed <= 0.0 {
        bail!("speed must be a finite number greater than zero");
    }
    if options.fps == 0 {
        bail!("fps must be greater than zero");
    }
    if options
        .idle_time_limit
        .is_some_and(|value| !valid_time(value))
    {
        bail!("idle time limit must be a finite, non-negative number");
    }
    for (name, value) in [
        ("at", options.at),
        ("from", options.from),
        ("to", options.to),
    ] {
        if value.is_some_and(|value| !valid_time(value)) {
            bail!("{name} must be a finite, non-negative number");
        }
    }
    if options.at.is_some() && (options.from.is_some() || options.to.is_some()) {
        bail!("at cannot be combined with from or to");
    }
    if let (Some(from), Some(to)) = (options.from, options.to)
        && from > to
    {
        bail!("from cannot be greater than to");
    }

    Ok(())
}

fn valid_time(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

fn push_visual_frame(frames: &mut Vec<Frame>, frame: Frame) {
    let Some(last) = frames.last_mut() else {
        frames.push(frame);
        return;
    };

    if last.snapshot.same_visual(&frame.snapshot) {
        return;
    }

    if last.time == frame.time {
        last.snapshot = frame.snapshot;
    } else {
        frames.push(frame);
    }
}

/// Keep the latest visual state in each FPS window. This bounds SVG growth
/// without manufacturing duplicate frames as fixed-rate resampling would.
fn cap_fps(frames: Vec<Frame>, fps: u16) -> Vec<Frame> {
    let interval = 1.0 / f64::from(fps);
    let mut input = frames.into_iter();
    let Some(mut held) = input.next() else {
        return Vec::new();
    };
    let mut output = Vec::new();

    for frame in input {
        if frame.time - held.time < interval {
            held.snapshot = frame.snapshot;
        } else {
            output.push(held);
            held = frame;
        }
    }
    output.push(held);

    output
}

fn select(
    frames: Vec<Frame>,
    cols: usize,
    rows: usize,
    full_duration: f64,
    options: &TimelineOptions,
) -> Result<Timeline> {
    if let Some(at) = options.at {
        let snapshot = state_at(&frames, at).clone();
        return Ok(Timeline {
            cols,
            rows,
            duration: 0.0,
            frames: vec![Frame {
                time: 0.0,
                snapshot,
            }],
        });
    }

    let from = options.from.unwrap_or(0.0);
    let to = options.to.unwrap_or(full_duration);
    if from > to {
        bail!("from cannot be greater than the effective end time");
    }

    let mut selected = vec![Frame {
        time: 0.0,
        snapshot: state_at(&frames, from).clone(),
    }];

    for frame in frames
        .into_iter()
        .filter(|frame| frame.time > from && frame.time <= to)
    {
        push_visual_frame(
            &mut selected,
            Frame {
                time: frame.time - from,
                snapshot: frame.snapshot,
            },
        );
    }

    Ok(Timeline {
        cols,
        rows,
        duration: to - from,
        frames: selected,
    })
}

fn state_at(frames: &[Frame], time: f64) -> &Snapshot {
    &frames
        .iter()
        .rev()
        .find(|frame| frame.time <= time)
        .unwrap_or_else(|| frames.first().expect("timeline always has a blank frame"))
        .snapshot
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::asciicast::Asciicast;

    use super::*;

    fn cast(events: &str) -> Asciicast {
        let input = format!("{{\"version\":3,\"term\":{{\"cols\":4,\"rows\":2}}}}\n{events}");
        Asciicast::parse(Cursor::new(input)).unwrap()
    }

    #[test]
    fn applies_output_after_each_relative_v3_interval() {
        let timeline = build(
            &cast("[0.5,\"o\",\"a\"]\n[1.0,\"o\",\"b\"]\n"),
            &TimelineOptions::default(),
        )
        .unwrap();

        assert_eq!(timeline.frames.len(), 3);
        assert_eq!(timeline.frames[1].time, 0.5);
        assert_eq!(timeline.frames[2].time, 1.5);
        assert!(
            timeline.frames[2].snapshot.lines[0]
                .text()
                .starts_with("ab")
        );
        assert_eq!(timeline.duration, 1.5);
    }

    #[test]
    fn applies_header_idle_limit_and_speed_to_the_whole_timeline() {
        let cast = Asciicast::parse(Cursor::new(
            "{\"version\":3,\"term\":{\"cols\":4,\"rows\":2},\"idle_time_limit\":2}\n[0.5,\"o\",\"a\"]\n[10,\"m\",\"pause\"]\n",
        ))
        .unwrap();
        let options = TimelineOptions {
            speed: 2.0,
            ..TimelineOptions::default()
        };
        let timeline = build(&cast, &options).unwrap();

        assert_eq!(timeline.duration, 1.25);
    }

    #[test]
    fn processes_resize_events_and_uses_the_largest_canvas() {
        let timeline = build(
            &cast("[0,\"o\",\"abcd\"]\n[1,\"r\",\"8x3\"]\n"),
            &TimelineOptions::default(),
        )
        .unwrap();

        assert_eq!((timeline.cols, timeline.rows), (8, 3));
        assert_eq!(timeline.frames.last().unwrap().snapshot.cols, 8);
        assert_eq!(timeline.frames.last().unwrap().snapshot.rows, 3);
    }

    #[test]
    fn dimension_overrides_pin_resize_axes() {
        let options = TimelineOptions {
            cols: Some(6),
            rows: Some(4),
            ..TimelineOptions::default()
        };
        let timeline = build(&cast("[1,\"r\",\"8x3\"]\n"), &options).unwrap();

        assert_eq!((timeline.cols, timeline.rows), (6, 4));
        assert_eq!(timeline.frames.last().unwrap().snapshot.cols, 6);
    }

    #[test]
    fn range_is_rebased_and_starts_with_the_prior_state() {
        let options = TimelineOptions {
            from: Some(1.5),
            to: Some(3.5),
            ..TimelineOptions::default()
        };
        let timeline = build(
            &cast("[1,\"o\",\"a\"]\n[1,\"o\",\"b\"]\n[1,\"o\",\"c\"]\n"),
            &options,
        )
        .unwrap();

        assert_eq!(timeline.frames[0].time, 0.0);
        assert!(timeline.frames[0].snapshot.lines[0].text().starts_with('a'));
        assert_eq!(timeline.frames[1].time, 0.5);
        assert_eq!(timeline.duration, 2.0);
    }

    #[test]
    fn at_produces_one_static_frame() {
        let options = TimelineOptions {
            at: Some(2.0),
            ..TimelineOptions::default()
        };
        let timeline = build(&cast("[1,\"o\",\"a\"]\n[1,\"o\",\"b\"]\n"), &options).unwrap();

        assert_eq!(timeline.duration, 0.0);
        assert_eq!(timeline.frames.len(), 1);
        assert!(
            timeline.frames[0].snapshot.lines[0]
                .text()
                .starts_with("ab")
        );
    }

    #[test]
    fn static_seek_is_not_shifted_earlier_by_fps_capping() {
        let options = TimelineOptions {
            at: Some(0.0),
            fps: 10,
            ..TimelineOptions::default()
        };
        let timeline = build(&cast("[0.01,\"o\",\"a\"]\n"), &options).unwrap();

        assert!(
            timeline.frames[0].snapshot.lines[0]
                .text()
                .trim()
                .is_empty()
        );
    }

    #[test]
    fn fps_cap_keeps_the_latest_state_in_a_window() {
        let options = TimelineOptions {
            fps: 10,
            ..TimelineOptions::default()
        };
        let timeline = build(
            &cast("[0.01,\"o\",\"a\"]\n[0.01,\"o\",\"b\"]\n[0.2,\"o\",\"c\"]\n"),
            &options,
        )
        .unwrap();

        assert_eq!(timeline.frames.len(), 2);
        assert!(
            timeline.frames[0].snapshot.lines[0]
                .text()
                .starts_with("ab")
        );
    }
}
