//! The iced application: state, messages, update, view, subscription.

use crate::backend::{self, ProbeInfo};
use iced::widget::{
    Column, button, checkbox, column, container, pick_list, progress_bar, row, scrollable, space,
    text, tooltip,
};
use iced::{Alignment, Element, Length, Subscription, Task, Theme, event, window};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use wasmffmpeg_core::{MediaKind, OutputFormat, ResizeSpec};

pub type JobId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Probing,
    Unsupported,
    Pending,
    Running,
    Done,
    Cancelled,
    Failed,
}

pub struct Row {
    pub id: JobId,
    pub input: PathBuf,
    pub kind: Option<MediaKind>,
    pub format: Option<OutputFormat>,
    pub status: Status,
    pub progress: f32,
    pub probe: Option<ProbeInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Fhd,
    Hd,
    Uhd,
}

impl Preset {
    pub const ALL: &[Preset] = &[Preset::Fhd, Preset::Hd, Preset::Uhd];

    pub fn spec(self, allow_upscale: bool) -> ResizeSpec {
        let (width, height) = match self {
            Preset::Fhd => (1920, 1080),
            Preset::Hd => (1280, 720),
            Preset::Uhd => (3840, 2160),
        };
        ResizeSpec::new(width, height, allow_upscale)
    }
}

impl std::fmt::Display for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Preset::Fhd => "1920×1080 (Full HD)",
            Preset::Hd => "1280×720 (HD)",
            Preset::Uhd => "3840×2160 (4K UHD)",
        })
    }
}

pub struct App {
    rows: Vec<Row>,
    next_id: JobId,
    out_dir: Option<PathBuf>,
    preset: Preset,
    allow_upscale: bool,
    ffmpeg_error: Option<String>,
    cancel_flags: HashMap<JobId, Arc<AtomicBool>>,
    running: Option<JobId>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    AddFiles,
    FilesAdded(Vec<PathBuf>),
    PickOutputDir,
    OutputDirPicked(Option<PathBuf>),
    FileDropped(PathBuf),
    Probed(JobId, Result<ProbeInfo, String>),
    SetRowFormat(JobId, OutputFormat),
    SetPreset(Preset),
    ToggleUpscale(bool),
    RemoveRow(JobId),
    ClearFinished,
    Start,
    JobProgress(JobId, f32),
    JobFinished(JobId, Result<PathBuf, backend::BackendError>),
    CancelJob(JobId),
    StopAll,
}

/// Application entry point called from `main`.
pub fn run() -> iced::Result {
    iced::application(boot, update, view)
        .subscription(subscription)
        .theme(Theme::Dark)
        .title("wasmffmpeg — media converter")
        .window_size([1100.0, 720.0])
        .centered()
        .run()
}

pub fn boot() -> (App, Task<Msg>) {
    let ffmpeg_error = backend::check_ffmpeg_available().err();
    (
        App {
            rows: Vec::new(),
            next_id: 0,
            out_dir: None,
            preset: Preset::Fhd,
            allow_upscale: false,
            ffmpeg_error,
            cancel_flags: HashMap::new(),
            running: None,
        },
        Task::none(),
    )
}

fn supported_extensions() -> Vec<&'static str> {
    wasmffmpeg_core::media::IMAGE_EXTENSIONS
        .iter()
        .chain(wasmffmpeg_core::media::VIDEO_EXTENSIONS)
        .copied()
        .collect()
}

fn update(app: &mut App, msg: Msg) -> Task<Msg> {
    match msg {
        Msg::AddFiles => Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_title("Add media files")
                    .add_filter("Media files", &supported_extensions())
                    .pick_files()
                    .await
                    .map(|handles| handles.iter().map(|h| h.path().to_path_buf()).collect())
                    .unwrap_or_default()
            },
            Msg::FilesAdded,
        ),
        Msg::FileDropped(path) => update(app, Msg::FilesAdded(vec![path])),
        Msg::FilesAdded(paths) => {
            let mut tasks = Vec::new();
            for path in paths {
                if app.rows.iter().any(|r| r.input == path) {
                    continue;
                }
                let kind = MediaKind::from_path(&path);
                let id = app.next_id;
                app.next_id += 1;
                let row = Row {
                    id,
                    input: path.clone(),
                    kind,
                    format: kind.map(OutputFormat::default_for),
                    status: if kind.is_some() {
                        Status::Probing
                    } else {
                        Status::Unsupported
                    },
                    progress: 0.0,
                    probe: None,
                    error: None,
                };
                app.rows.push(row);
                if kind.is_some() {
                    tasks.push(Task::perform(
                        {
                            let path = path.clone();
                            async move { backend::native::probe(&path).map_err(|e| e.to_string()) }
                        },
                        move |result| Msg::Probed(id, result),
                    ));
                }
            }
            Task::batch(tasks)
        }
        Msg::Probed(id, result) => {
            if let Some(row) = app.rows.iter_mut().find(|r| r.id == id) {
                match result {
                    Ok(info) => {
                        row.probe = Some(info);
                        row.status = Status::Pending;
                    }
                    Err(e) => {
                        row.error = Some(e);
                        row.status = Status::Failed;
                    }
                }
            }
            Task::none()
        }
        Msg::PickOutputDir => Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_title("Choose output folder")
                    .pick_folder()
                    .await
                    .map(|h| h.path().to_path_buf())
            },
            Msg::OutputDirPicked,
        ),
        Msg::OutputDirPicked(dir) => {
            app.out_dir = dir;
            Task::none()
        }
        Msg::SetRowFormat(id, format) => {
            if let Some(row) = app.rows.iter_mut().find(|r| r.id == id) {
                row.format = Some(format);
            }
            Task::none()
        }
        Msg::SetPreset(preset) => {
            app.preset = preset;
            Task::none()
        }
        Msg::ToggleUpscale(allow) => {
            app.allow_upscale = allow;
            Task::none()
        }
        Msg::RemoveRow(id) => {
            if app.running != Some(id) {
                app.rows.retain(|r| r.id != id);
            }
            Task::none()
        }
        Msg::ClearFinished => {
            app.rows
                .retain(|r| !matches!(r.status, Status::Done | Status::Cancelled));
            Task::none()
        }
        Msg::Start => {
            if app.running.is_some() {
                return Task::none();
            }
            start_next(app)
        }
        Msg::JobProgress(id, progress) => {
            if let Some(row) = app.rows.iter_mut().find(|r| r.id == id) {
                row.progress = progress;
            }
            Task::none()
        }
        Msg::JobFinished(id, result) => {
            app.cancel_flags.remove(&id);
            if app.running == Some(id) {
                app.running = None;
            }
            if let Some(row) = app.rows.iter_mut().find(|r| r.id == id) {
                match result {
                    Ok(_) => {
                        row.status = Status::Done;
                        row.progress = 1.0;
                    }
                    Err(backend::BackendError::Cancelled) => {
                        row.status = Status::Cancelled;
                    }
                    Err(e) => {
                        row.status = Status::Failed;
                        row.error = Some(e.to_string());
                    }
                }
            }
            start_next(app)
        }
        Msg::CancelJob(id) => {
            if let Some(flag) = app.cancel_flags.get(&id) {
                flag.store(true, Ordering::Relaxed);
            }
            Task::none()
        }
        Msg::StopAll => {
            for flag in app.cancel_flags.values() {
                flag.store(true, Ordering::Relaxed);
            }
            for row in app.rows.iter_mut() {
                if row.status == Status::Pending {
                    row.status = Status::Cancelled;
                }
            }
            Task::none()
        }
    }
}

impl App {
    fn effective_out_dir(&self) -> Option<PathBuf> {
        self.out_dir.clone().or_else(|| {
            self.rows
                .iter()
                .find_map(|r| r.input.parent().map(|p| p.join("converted")))
        })
    }
}

/// Starts the next pending job, if any. Rows whose output directory cannot
/// be created fail immediately and the queue moves on. Exactly one ffmpeg
/// runs at a time.
fn start_next(app: &mut App) -> Task<Msg> {
    let Some(out_dir) = app.effective_out_dir() else {
        return Task::none();
    };
    let Some(index) = app
        .rows
        .iter()
        .position(|r| r.status == Status::Pending && r.probe.is_some())
    else {
        app.running = None;
        return Task::none();
    };

    let row = &mut app.rows[index];
    let id = row.id;
    let probe = row.probe.expect("checked above");
    let format = row.format.expect("pending rows always have a format");

    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        row.status = Status::Failed;
        row.error = Some(format!("cannot create output folder: {e}"));
        return start_next(app);
    }

    let output = backend::native::unique_output_path(&out_dir, &row.input, format);
    row.status = Status::Running;
    row.progress = 0.0;

    let flag = Arc::new(AtomicBool::new(false));
    app.cancel_flags.insert(id, flag.clone());
    app.running = Some(id);

    let job = wasmffmpeg_core::ConversionJob {
        input: row.input.clone(),
        output,
        format,
        input_width: probe.width,
        input_height: probe.height,
        resize: app.preset.spec(app.allow_upscale),
    };
    let duration = probe.duration_secs;
    let sipper = backend::native::convert(job, duration, flag);
    Task::sip(
        sipper,
        move |progress| Msg::JobProgress(id, progress),
        move |output| Msg::JobFinished(id, output),
    )
}

fn subscription(_app: &App) -> Subscription<Msg> {
    event::listen().filter_map(|event| match event {
        iced::Event::Window(window::Event::FileDropped(path)) => Some(Msg::FileDropped(path)),
        _ => None,
    })
}

fn view(app: &App) -> Element<'_, Msg> {
    let mut content = column![].spacing(12);

    if let Some(error) = &app.ffmpeg_error {
        content = content.push(
            container(text(error).size(14))
                .padding(12)
                .width(Length::Fill)
                .style(container::danger),
        );
    }

    let header = row![
        text("wasmffmpeg").size(24),
        space().width(Length::Fill),
        button("Add files").on_press(Msg::AddFiles),
        button("Output folder…").on_press(Msg::PickOutputDir),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let controls = row![
        text("Size:"),
        pick_list(Preset::ALL, Some(app.preset), Msg::SetPreset),
        checkbox(app.allow_upscale)
            .label("Allow upscale")
            .on_toggle(Msg::ToggleUpscale),
        space().width(Length::Fill),
        text(format!(
            "Output: {}",
            app.effective_out_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "—".into())
        ))
        .size(13),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let pending = app
        .rows
        .iter()
        .filter(|r| r.status == Status::Pending)
        .count();
    let mut start_button = button("Convert");
    if pending > 0 && app.ffmpeg_error.is_none() && app.running.is_none() {
        start_button = start_button.on_press(Msg::Start);
    }
    let mut stop_button = button("Stop all");
    if app.running.is_some() {
        stop_button = stop_button.on_press(Msg::StopAll);
    }
    let actions = row![
        start_button,
        stop_button,
        button("Clear finished").on_press(Msg::ClearFinished),
        text(format!("{} file(s), {} ready", app.rows.len(), pending)).size(13),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let list = app
        .rows
        .iter()
        .fold(Column::new().spacing(8), |col, row| col.push(view_row(row)));

    content = content
        .push(header)
        .push(controls)
        .push(actions)
        .push(scrollable(list));
    container(content).padding(16).into()
}

fn view_row(row: &Row) -> Element<'_, Msg> {
    let id = row.id;
    let name = row
        .input
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string();

    let kind_label = match row.kind {
        Some(MediaKind::Image) => "image",
        Some(MediaKind::Video) => "video",
        None => "unsupported",
    };

    let mut line = row![
        text(name).width(Length::FillPortion(3)),
        text(kind_label).width(Length::FillPortion(1)),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    if let (Some(kind), Some(format)) = (row.kind, row.format) {
        line = line.push(
            pick_list(OutputFormat::all_for(kind), Some(format), move |f| {
                Msg::SetRowFormat(id, f)
            })
            .width(Length::FillPortion(2)),
        );
    } else {
        line = line.push(text("—").width(Length::FillPortion(2)));
    }

    let status_widget: Element<'_, Msg> = match row.status {
        Status::Probing => text("Probing…").size(13).into(),
        Status::Unsupported => text("Unsupported type").size(13).into(),
        Status::Pending => {
            let dims = row
                .probe
                .map(|p| format!("{}×{}", p.width, p.height))
                .unwrap_or_default();
            text(format!("Ready {dims}")).size(13).into()
        }
        Status::Running if row.kind == Some(MediaKind::Image) => {
            text("Processing…").size(13).into()
        }
        Status::Running => progress_bar(0.0..=1.0, row.progress).into(),
        Status::Done => text("Done").size(13).into(),
        Status::Cancelled => text("Cancelled").size(13).into(),
        Status::Failed => tooltip(
            text("Failed").size(13),
            text(row.error.clone().unwrap_or_default()).size(12),
            tooltip::Position::Bottom,
        )
        .into(),
    };
    line = line.push(container(status_widget).width(Length::FillPortion(2)));

    if row.status == Status::Running {
        line = line.push(button("Cancel").on_press(Msg::CancelJob(id)));
    } else {
        line = line.push(button("Remove").on_press(Msg::RemoveRow(id)));
    }

    line.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_specs() {
        assert_eq!(Preset::Fhd.spec(false), ResizeSpec::new(1920, 1080, false));
        assert_eq!(Preset::Hd.spec(true), ResizeSpec::new(1280, 720, true));
        assert_eq!(Preset::Uhd.spec(false), ResizeSpec::new(3840, 2160, false));
    }
}
