//! F11 records the game to `captures/clip-*.mp4` until it is pressed again, from the main menu
//! into a game and back: H.264 at up to 30 frames a second, encoded on its own thread through
//! Windows Media Foundation (on the graphics card when it can). A dot in the corner shows for a
//! moment when recording starts (red) and when the clip is saved (green), and stays out of the clip.
use bevy::{
    app::AppExit,
    prelude::*,
    render::{render_resource::TextureFormat, view::screenshot::{Screenshot, ScreenshotCaptured}},
};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::{AtomicBool, AtomicU32, Ordering}, mpsc, Arc},
    thread::JoinHandle,
    time::Duration,
};

/// Frames a second the clip is captured at.
const FPS: u32 = 30;
/// Captured frames waiting for the encoder before new ones are dropped.
const BACKLOG: usize = 8;
/// How long the corner dot shows.
const BADGE_SECONDS: f32 = 0.5;
/// The first frame is taken once the start dot is gone.
const START_DELAY: f32 = 0.6;

pub struct RecordingPlugin;
impl Plugin for RecordingPlugin {
    fn build(&self, app: &mut App) {
        let cli = app.world().resource::<crate::cli::Cli>();
        if cli.is_scripted() {
            return;
        }
        app.init_resource::<Recorder>().add_systems(Update, (toggle, capture, finish).chain()).add_systems(Last, stop_on_exit);
    }
}

/// One frame for the encoder: its time in the clip, in seconds, and the picture.
struct Frame {
    at: f64,
    image: Image,
}

/// What a finished clip holds.
struct Clip {
    frames: u32,
    seconds: f64,
}

struct Live {
    tx: mpsc::SyncSender<Frame>,
    stop: Arc<AtomicBool>,
    dropped: Arc<AtomicU32>,
    worker: JoinHandle<Result<Clip, String>>,
    path: PathBuf,
    /// Real time the clip starts at, and the next frame is due at.
    started: f32,
    next: f32,
}

struct Finishing {
    stop: Arc<AtomicBool>,
    dropped: Arc<AtomicU32>,
    worker: JoinHandle<Result<Clip, String>>,
    path: PathBuf,
}

#[derive(Resource, Default)]
struct Recorder {
    live: Option<Live>,
    finishing: Vec<Finishing>,
    badge: Option<(Entity, f32)>,
}

impl Recorder {
    fn stop(&mut self) {
        if let Some(live) = self.live.take() {
            live.stop.store(true, Ordering::Release);
            sim::trace::event(format!("RECORD_STOP {}", live.path.display()));
            self.finishing.push(Finishing { stop: live.stop, dropped: live.dropped, worker: live.worker, path: live.path });
        }
    }
    /// Wait for every clip to be written (the game is closing).
    fn finish_all(&mut self) {
        self.stop();
        for f in self.finishing.drain(..) {
            f.stop.store(true, Ordering::Release);
            match f.worker.join() {
                Ok(Ok(clip)) => info!("video saved to {} ({} frames, {:.1} s)", f.path.display(), clip.frames, clip.seconds),
                Ok(Err(e)) => error!("video {} was not saved: {e}", f.path.display()),
                Err(_) => error!("video {} was not saved: the encoder crashed", f.path.display()),
            }
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.finish_all();
    }
}

fn badge(commands: &mut Commands, rec: &mut Recorder, color: Color, now: f32) {
    if let Some((e, _)) = rec.badge.take() {
        commands.entity(e).despawn();
    }
    let e = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(14.0),
                right: Val::Px(14.0),
                width: Val::Px(16.0),
                height: Val::Px(16.0),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(color),
            GlobalZIndex(i32::MAX),
        ))
        .id();
    rec.badge = Some((e, now + BADGE_SECONDS));
}

fn toggle(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>, time: Res<Time<Real>>, mut rec: ResMut<Recorder>) {
    if !keys.just_pressed(KeyCode::F11) {
        return;
    }
    let now = time.elapsed_secs();
    if rec.live.is_some() {
        rec.stop();
        return;
    }
    let dir = crate::input_trace::captures();
    let path = dir.join(format!("clip-{}.mp4", crate::input_trace::stamp()));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        error!("video not started: {e}");
        badge(&mut commands, &mut rec, crate::hud::BAD, now);
        return;
    }
    let (tx, rx) = mpsc::sync_channel(BACKLOG);
    let stop = Arc::new(AtomicBool::new(false));
    let worker = {
        let (stop, path) = (stop.clone(), path.clone());
        std::thread::Builder::new().name("video".into()).spawn(move || encode(rx, &stop, &path))
    };
    let worker = match worker {
        Ok(w) => w,
        Err(e) => {
            error!("video not started: {e}");
            badge(&mut commands, &mut rec, crate::hud::BAD, now);
            return;
        }
    };
    sim::trace::event(format!("RECORD_START {}", path.display()));
    info!("recording video to {}", path.display());
    let started = now + START_DELAY;
    rec.live = Some(Live { tx, stop, dropped: Arc::default(), worker, path, started, next: started });
    badge(&mut commands, &mut rec, crate::hud::BAD, now);
}

fn capture(mut commands: Commands, time: Res<Time<Real>>, mut rec: ResMut<Recorder>) {
    let now = time.elapsed_secs();
    if rec.badge.is_some_and(|(_, until)| now >= until) {
        let (e, _) = rec.badge.take().unwrap();
        commands.entity(e).despawn();
    }
    let Some(live) = rec.live.as_mut() else { return };
    if now < live.next {
        return;
    }
    let step = 1.0 / FPS as f32;
    // After a long frame (a game loading), carry on from now rather than catching up.
    live.next = if now - live.next > step { now + step } else { live.next + step };
    let at = f64::from(now - live.started);
    let (tx, dropped) = (live.tx.clone(), live.dropped.clone());
    commands.spawn(Screenshot::primary_window()).observe(move |shot: On<ScreenshotCaptured>| {
        if tx.try_send(Frame { at, image: shot.image.clone() }).is_err() {
            dropped.fetch_add(1, Ordering::Relaxed);
        }
    });
}

fn finish(mut commands: Commands, time: Res<Time<Real>>, mut rec: ResMut<Recorder>, mut hud: Option<ResMut<crate::hud::HudState>>) {
    let now = time.elapsed_secs();
    let mut i = 0;
    while i < rec.finishing.len() {
        if !rec.finishing[i].worker.is_finished() {
            i += 1;
            continue;
        }
        let f = rec.finishing.remove(i);
        let dropped = f.dropped.load(Ordering::Relaxed);
        let (color, msg) = match f.worker.join() {
            Ok(Ok(clip)) => {
                info!("video saved to {} ({} frames, {:.1} s, {dropped} frames skipped)", f.path.display(), clip.frames, clip.seconds);
                sim::trace::event(format!("RECORD_SAVED {} frames={} seconds={:.1} skipped={dropped}", f.path.display(), clip.frames, clip.seconds));
                (crate::hud::GOOD, crate::hud::Msg { icon: Some((crate::icons::Icon::Check, crate::hud::GOOD)), icon2: None, text: String::new() })
            }
            failed => {
                let why = match failed { Ok(Err(e)) => e, _ => "the encoder crashed".into() };
                error!("video {} was not saved: {why}", f.path.display());
                sim::trace::event(format!("RECORD_FAILED {} {why}", f.path.display()));
                (crate::hud::BAD, crate::hud::Msg { icon: Some((crate::icons::Icon::Cross, crate::hud::BAD)), icon2: None, text: format!("Video not saved: {why}") })
            }
        };
        badge(&mut commands, &mut rec, color, now);
        if let Some(hud) = hud.as_mut() {
            hud.msg = Some((msg, time.elapsed_secs()));
        }
    }
}

fn stop_on_exit(mut exits: MessageReader<AppExit>, mut rec: ResMut<Recorder>) {
    if exits.read().next().is_some() {
        rec.finish_all();
    }
}

/// Encode frames as they come until told to stop, then write out the file.
fn encode(rx: mpsc::Receiver<Frame>, stop: &AtomicBool, path: &Path) -> Result<Clip, String> {
    let mut writer: Option<mf::Writer> = None;
    let mut pixels = Vec::new();
    let (mut frames, mut last) = (0u32, -1i64);
    loop {
        let frame = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(f) => f,
            // Frames asked for before the stop arrive a few frames later; a quiet spell ends the clip.
            Err(mpsc::RecvTimeoutError::Timeout) if stop.load(Ordering::Acquire) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let (w, h) = (frame.image.width(), frame.image.height());
        if !bgra(frame.image, &mut pixels) {
            continue;
        }
        let w_out = writer.get_or_insert_with_result(|| mf::Writer::create(path, w & !1, h & !1, FPS))?;
        // A window resized mid-clip: keep the size the clip started with.
        if w_out.width() > w || w_out.height() > h {
            continue;
        }
        // 100 ns units, strictly increasing.
        let t = ((frame.at * 1e7) as i64).max(last + 1);
        w_out.write(&pixels, w, t, 10_000_000 / i64::from(FPS))?;
        last = t;
        frames += 1;
    }
    let Some(w) = writer else { return Err("no frames were captured".into()) };
    w.finish()?;
    Ok(Clip { frames, seconds: last.max(0) as f64 / 1e7 + 1.0 / f64::from(FPS) })
}

trait GetOrInsertResult<T> {
    fn get_or_insert_with_result<E>(&mut self, f: impl FnOnce() -> Result<T, E>) -> Result<&mut T, E>;
}
impl<T> GetOrInsertResult<T> for Option<T> {
    fn get_or_insert_with_result<E>(&mut self, f: impl FnOnce() -> Result<T, E>) -> Result<&mut T, E> {
        if self.is_none() {
            *self = Some(f()?);
        }
        Ok(self.as_mut().unwrap())
    }
}

/// The picture as tightly packed 8-bit BGRA rows, top row first. False when it can't be read.
fn bgra(image: Image, out: &mut Vec<u8>) -> bool {
    let format = image.texture_descriptor.format;
    let n = image.width() as usize * image.height() as usize * 4;
    match format {
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb | TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => {
            let Some(data) = image.data else { return false };
            if data.len() < n {
                return false;
            }
            *out = data;
            out.truncate(n);
            if matches!(format, TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb) {
                swap_red_blue(out);
            }
        }
        _ => {
            let Ok(img) = image.try_into_dynamic() else { return false };
            *out = img.to_rgba8().into_raw();
            swap_red_blue(out);
        }
    }
    true
}

fn swap_red_blue(px: &mut [u8]) {
    for p in px.chunks_exact_mut(4) {
        p.swap(0, 2);
    }
}

#[cfg(windows)]
mod mf {
    use std::path::Path;
    use windows::{
        core::{Result, HSTRING},
        Win32::{Media::MediaFoundation::*, System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED}},
    };

    /// An MP4 file being written, H.264 from 32-bit RGB frames.
    pub struct Writer {
        sink: IMFSinkWriter,
        stream: u32,
        w: u32,
        h: u32,
    }

    fn pair(a: u32, b: u32) -> u64 {
        (u64::from(a) << 32) | u64::from(b)
    }

    impl Writer {
        pub fn create(path: &Path, w: u32, h: u32, fps: u32) -> std::result::Result<Writer, String> {
            if w < 2 || h < 2 {
                return Err(format!("the window is too small to record ({w}x{h})"));
            }
            unsafe {
                // Fails harmlessly when this thread already has COM.
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(|e| format!("Media Foundation is unavailable: {e}"))?;
            }
            // The graphics card's encoder first, then Windows' own.
            let made = Self::open(path, w, h, fps, true).or_else(|_| Self::open(path, w, h, fps, false));
            made.map_err(|e| {
                unsafe {
                    let _ = MFShutdown();
                    CoUninitialize();
                }
                format!("the video encoder would not start: {e}")
            })
        }

        fn open(path: &Path, w: u32, h: u32, fps: u32, hardware: bool) -> Result<Writer> {
            unsafe {
                let mut attrs = None;
                MFCreateAttributes(&mut attrs, 2)?;
                let attrs = attrs.expect("MFCreateAttributes returned no attributes");
                attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, u32::from(hardware))?;
                attrs.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4)?;
                let sink = MFCreateSinkWriterFromURL(&HSTRING::from(path.as_os_str()), None::<&IMFByteStream>, &attrs)?;

                let bitrate = (f64::from(w) * f64::from(h) * f64::from(fps) * 0.15).min(60e6) as u32;
                let out = MFCreateMediaType()?;
                out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
                out.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
                out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
                out.SetUINT64(&MF_MT_FRAME_SIZE, pair(w, h))?;
                out.SetUINT64(&MF_MT_FRAME_RATE, pair(fps, 1))?;
                out.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pair(1, 1))?;
                let stream = sink.AddStream(&out)?;

                let input = MFCreateMediaType()?;
                input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
                input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
                input.SetUINT64(&MF_MT_FRAME_SIZE, pair(w, h))?;
                input.SetUINT64(&MF_MT_FRAME_RATE, pair(fps, 1))?;
                input.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pair(1, 1))?;
                sink.SetInputMediaType(stream, &input, None::<&IMFAttributes>)?;
                sink.BeginWriting()?;
                Ok(Writer { sink, stream, w, h })
            }
        }

        pub fn width(&self) -> u32 { self.w }
        pub fn height(&self) -> u32 { self.h }

        /// One frame of packed BGRA rows `src_w` pixels wide, top row first, at `time` for `duration`.
        pub fn write(&mut self, bgra: &[u8], src_w: u32, time: i64, duration: i64) -> std::result::Result<(), String> {
            self.write_inner(bgra, src_w, time, duration).map_err(|e| format!("a frame could not be encoded: {e}"))
        }

        fn write_inner(&mut self, bgra: &[u8], src_w: u32, time: i64, duration: i64) -> Result<()> {
            let row = self.w as usize * 4;
            let src_row = src_w as usize * 4;
            let len = row * self.h as usize;
            unsafe {
                let buf = MFCreateMemoryBuffer(len as u32)?;
                let mut p = std::ptr::null_mut();
                buf.Lock(&mut p, None, None)?;
                let dst = std::slice::from_raw_parts_mut(p, len);
                // 32-bit RGB frames are stored bottom row first.
                for (y, out) in dst.chunks_exact_mut(row).enumerate() {
                    let src = (self.h as usize - 1 - y) * src_row;
                    out.copy_from_slice(&bgra[src..src + row]);
                }
                buf.Unlock()?;
                buf.SetCurrentLength(len as u32)?;
                let sample = MFCreateSample()?;
                sample.AddBuffer(&buf)?;
                sample.SetSampleTime(time)?;
                sample.SetSampleDuration(duration)?;
                self.sink.WriteSample(self.stream, &sample)
            }
        }

        pub fn finish(self) -> std::result::Result<(), String> {
            let done = unsafe { self.sink.Finalize() };
            drop(self.sink);
            unsafe {
                let _ = MFShutdown();
                CoUninitialize();
            }
            done.map_err(|e| format!("the file could not be finished: {e}"))
        }
    }
}

#[cfg(not(windows))]
mod mf {
    use std::path::Path;

    pub struct Writer;
    impl Writer {
        pub fn create(_: &Path, _: u32, _: u32, _: u32) -> Result<Writer, String> {
            Err("video recording needs Windows".into())
        }
        pub fn width(&self) -> u32 { 0 }
        pub fn height(&self) -> u32 { 0 }
        pub fn write(&mut self, _: &[u8], _: u32, _: i64, _: i64) -> Result<(), String> { Ok(()) }
        pub fn finish(self) -> Result<(), String> { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_frames_become_bgra() {
        let mut image = Image::new_fill(
            bevy::render::render_resource::Extent3d { width: 2, height: 1, depth_or_array_layers: 1 },
            bevy::render::render_resource::TextureDimension::D2,
            &[10, 20, 30, 255],
            TextureFormat::Rgba8UnormSrgb,
            Default::default(),
        );
        image.data.as_mut().unwrap()[4] = 40;
        let mut out = Vec::new();
        assert!(bgra(image, &mut out));
        assert_eq!(out, vec![30, 20, 10, 255, 30, 20, 40, 255]);
    }

    /// A short clip written and finished on this machine's encoder.
    #[cfg(windows)]
    #[test]
    fn writes_a_playable_mp4() {
        let dir = std::env::temp_dir().join(format!("beaver-video-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clip.mp4");
        let (tx, rx) = mpsc::sync_channel(BACKLOG);
        let stop = AtomicBool::new(true);
        let (w, h) = (320u32, 240u32);
        for i in 0..6 {
            let mut image = Image::new_fill(
                bevy::render::render_resource::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                bevy::render::render_resource::TextureDimension::D2,
                &[(i * 40) as u8, 100, 200, 255],
                TextureFormat::Bgra8UnormSrgb,
                Default::default(),
            );
            image.data.as_mut().unwrap().truncate((w * h * 4) as usize);
            tx.send(Frame { at: f64::from(i) / f64::from(FPS), image }).unwrap();
        }
        drop(tx);
        let clip = encode(rx, &stop, &path).unwrap();
        assert_eq!(clip.frames, 6);
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.len() > 100 && &bytes[4..8] == b"ftyp", "not an MP4 file");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
