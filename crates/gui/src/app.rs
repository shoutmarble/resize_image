//! The iced application: state, messages, update, view, subscription.

use crate::backend::{self, ProbeInfo};
use crate::settings::{self, Preset, Settings};
use iced::widget::{
    Column, button, checkbox, column, container, pick_list, progress_bar, row, scrollable, space,
    text, tooltip,
};
use iced::{Alignment, Element, Length, Subscription, Task, Theme, event, window};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use wasmffmpeg_core::{ImageFormat, MediaKind, OutputFormat, VideoFormat};

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

pub struct App {
    rows: Vec<Row>,
    next_id: JobId,
    out_dir: Option<PathBuf>,
    image_format: ImageFormat,
    video_format: VideoFormat,
    preset: Preset,
    allow_upscale: bool,
    ffmpeg_error: Option<String>,
    cancel_flags: HashMap<JobId, Arc<AtomicBool>>,
    running: Option<JobId>,
    drag_hover: bool,
}

#[derive(Debug, Clone)]
pub enum Msg {
    AddFiles,
    FilesAdded(Vec<PathBuf>),
    PickOutputDir,
    OutputDirPicked(Option<PathBuf>),
    FileDropped(PathBuf),
    DragHover(bool),
    Probed(JobId, Result<ProbeInfo, String>),
    SetRowFormat(JobId, OutputFormat),
    SetDefaultImageFormat(ImageFormat),
    SetDefaultVideoFormat(VideoFormat),
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
    let settings = settings::load();
    (
        App {
            rows: Vec::new(),
            next_id: 0,
            out_dir: settings.out_dir,
            image_format: settings.image_format,
            video_format: settings.video_format,
            preset: settings.preset,
            allow_upscale: settings.allow_upscale,
            ffmpeg_error,
            cancel_flags: HashMap::new(),
            running: None,
            drag_hover: false,
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
        Msg::FileDropped(path) => {
            app.drag_hover = false;
            update(app, Msg::FilesAdded(vec![path]))
        }
        Msg::DragHover(hovering) => {
            app.drag_hover = hovering;
            Task::none()
        }
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
                    format: kind.map(|k| match k {
                        MediaKind::Image => OutputFormat::Image(app.image_format),
                        MediaKind::Video => OutputFormat::Video(app.video_format),
                    }),
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
            let mut became_pending = false;
            if let Some(row) = app.rows.iter_mut().find(|r| r.id == id) {
                match result {
                    Ok(info) => {
                        row.probe = Some(info);
                        row.status = Status::Pending;
                        became_pending = true;
                    }
                    Err(e) => {
                        row.error = Some(e);
                        row.status = Status::Failed;
                    }
                }
            }
            // Files start converting as soon as they are ready; if the queue
            // is already busy they simply wait their turn.
            if became_pending {
                start_next(app)
            } else {
                Task::none()
            }
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
            app.persist();
            Task::none()
        }
        Msg::SetRowFormat(id, format) => {
            if let Some(row) = app.rows.iter_mut().find(|r| r.id == id) {
                row.format = Some(format);
            }
            Task::none()
        }
        Msg::SetDefaultImageFormat(format) => {
            app.image_format = format;
            app.persist();
            Task::none()
        }
        Msg::SetDefaultVideoFormat(format) => {
            app.video_format = format;
            app.persist();
            Task::none()
        }
        Msg::SetPreset(preset) => {
            app.preset = preset;
            app.persist();
            Task::none()
        }
        Msg::ToggleUpscale(allow) => {
            app.allow_upscale = allow;
            app.persist();
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
        Msg::Start => start_next(app),
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
    /// Where this row's output lands: the user-chosen folder when set,
    /// otherwise the input file's own folder.
    fn out_dir_for(&self, row: &Row) -> Option<PathBuf> {
        self.out_dir.clone().or_else(|| {
            row.input.parent().map(|p| {
                if p.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    p.to_path_buf()
                }
            })
        })
    }

    /// Saves the current preferences; failures are logged inside
    /// `settings::save` and never interrupt the session.
    fn persist(&self) {
        settings::save(&Settings {
            out_dir: self.out_dir.clone(),
            image_format: self.image_format,
            video_format: self.video_format,
            preset: self.preset,
            allow_upscale: self.allow_upscale,
        });
    }
}

/// Starts the next pending job, if any and none is running. Rows whose
/// output directory cannot be created fail immediately and the queue moves
/// on. Exactly one ffmpeg runs at a time.
fn start_next(app: &mut App) -> Task<Msg> {
    if app.running.is_some() {
        return Task::none();
    }
    let Some(index) = app
        .rows
        .iter()
        .position(|r| r.status == Status::Pending && r.probe.is_some())
    else {
        app.running = None;
        return Task::none();
    };

    let Some(out_dir) = app.out_dir_for(&app.rows[index]) else {
        let row = &mut app.rows[index];
        row.status = Status::Failed;
        row.error = Some("cannot determine output folder".into());
        return start_next(app);
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

    let output = backend::native::unique_output_path(
        &out_dir,
        &row.input,
        format,
        &backend::native::timestamp_now(),
    );
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
        iced::Event::Window(window::Event::FileHovered(_)) => Some(Msg::DragHover(true)),
        iced::Event::Window(window::Event::FileDropped(path)) => Some(Msg::FileDropped(path)),
        iced::Event::Window(window::Event::FilesHoveredLeft) => Some(Msg::DragHover(false)),
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
        text("Images →"),
        pick_list(
            ImageFormat::ALL,
            Some(app.image_format),
            Msg::SetDefaultImageFormat
        ),
        text("Videos →"),
        pick_list(
            VideoFormat::ALL,
            Some(app.video_format),
            Msg::SetDefaultVideoFormat
        ),
        space().width(Length::Fill),
        text(format!(
            "Output: {}",
            app.out_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "same folder as each input".into())
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

    let hovering = app.drag_hover;
    let drop_zone = container(
        text(if hovering {
            "Release to add — conversion starts immediately"
        } else {
            "Drag & drop images/videos anywhere here — conversion starts automatically"
        })
        .size(15),
    )
    .width(Length::Fill)
    .padding(28)
    .center_x(Length::Fill)
    .style(move |theme: &Theme| drop_zone_style(theme, hovering));

    content = content
        .push(header)
        .push(controls)
        .push(actions)
        .push(drop_zone)
        .push(scrollable(list));
    container(content).padding(16).into()
}

/// Highlights the drop zone while files hover over the window.
fn drop_zone_style(theme: &Theme, hovering: bool) -> container::Style {
    let palette = theme.extended_palette();
    let (border_color, background) = if hovering {
        (palette.primary.strong.color, palette.primary.weak.color)
    } else {
        (
            palette.background.strong.color,
            palette.background.weak.color,
        )
    };
    container::Style {
        border: iced::Border {
            color: border_color,
            width: 2.0,
            radius: 8.0.into(),
        },
        background: Some(iced::Background::Color(background)),
        ..Default::default()
    }
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
    use std::path::Path;

    fn app() -> App {
        App {
            rows: Vec::new(),
            next_id: 0,
            out_dir: None,
            image_format: ImageFormat::Jpeg,
            video_format: VideoFormat::Mp4H264,
            preset: Preset::Fhd,
            allow_upscale: false,
            ffmpeg_error: None,
            cancel_flags: HashMap::new(),
            running: None,
            drag_hover: false,
        }
    }

    fn row(input: &str) -> Row {
        Row {
            id: 0,
            input: PathBuf::from(input),
            kind: MediaKind::from_path(Path::new(input)),
            format: None,
            status: Status::Pending,
            progress: 0.0,
            probe: None,
            error: None,
        }
    }

    #[test]
    fn out_dir_defaults_to_input_folder() {
        let app = app();
        assert_eq!(
            app.out_dir_for(&row("/media/clip.mp4")),
            Some(PathBuf::from("/media"))
        );
    }

    #[test]
    fn chosen_out_dir_wins_over_input_folder() {
        let mut app = app();
        app.out_dir = Some(PathBuf::from("/chosen"));
        assert_eq!(
            app.out_dir_for(&row("/media/clip.mp4")),
            Some(PathBuf::from("/chosen"))
        );
    }

    #[test]
    fn bare_filename_falls_back_to_current_dir() {
        let app = app();
        assert_eq!(app.out_dir_for(&row("clip.mp4")), Some(PathBuf::from(".")));
    }

    #[test]
    fn added_files_get_persisted_default_formats() {
        let mut app = app();
        app.image_format = ImageFormat::WebP;
        app.video_format = VideoFormat::WebMAv1;
        let _ = update(
            &mut app,
            Msg::FilesAdded(vec![PathBuf::from("/a/photo.png")]),
        );
        let _ = update(
            &mut app,
            Msg::FilesAdded(vec![PathBuf::from("/a/clip.mkv")]),
        );
        assert_eq!(
            app.rows[0].format,
            Some(OutputFormat::Image(ImageFormat::WebP))
        );
        assert_eq!(
            app.rows[1].format,
            Some(OutputFormat::Video(VideoFormat::WebMAv1))
        );
    }

    #[test]
    fn start_next_is_a_no_op_while_running() {
        let mut app = app();
        app.running = Some(42);
        let _ = update(&mut app, Msg::Start);
        assert_eq!(app.running, Some(42));
        assert!(app.rows.is_empty());
    }

    #[test]
    fn drag_hover_toggles_and_drop_clears_it() {
        let mut app = app();
        let _ = update(&mut app, Msg::DragHover(true));
        assert!(app.drag_hover);
        let _ = update(&mut app, Msg::FileDropped(PathBuf::from("/a/photo.png")));
        assert!(!app.drag_hover);
        assert_eq!(app.rows.len(), 1);
    }
}
