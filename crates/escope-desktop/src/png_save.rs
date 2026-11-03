//! Local export is independent of the device connection and its worker.
use eframe::egui;
use escope_host::Capture;
use std::{
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
};

#[derive(Default)]
pub enum SaveStatus {
    #[default]
    Idle,
    Saving(PathBuf),
    Saved(PathBuf),
    Failed {
        path: PathBuf,
        error: String,
    },
}

struct Pending {
    path: PathBuf,
    completion: mpsc::Receiver<Result<(), String>>,
}

#[derive(Default)]
pub struct PngSaver {
    pending: Option<Pending>,
    status: SaveStatus,
}

impl PngSaver {
    pub fn is_saving(&self) -> bool {
        self.pending.is_some()
    }

    pub fn status(&self) -> &SaveStatus {
        &self.status
    }

    pub fn start(&mut self, capture: Arc<Capture>, path: PathBuf, ctx: egui::Context) -> bool {
        self.start_with(path, ctx, move |path| {
            // Keep the host SDK's atomic create_new contract. Even an existence
            // check can block on a mounted volume, so all file access stays here.
            capture.save_png(path).map_err(|error| error.to_string())
        })
    }

    fn start_with(
        &mut self,
        path: PathBuf,
        ctx: egui::Context,
        save: impl FnOnce(&Path) -> Result<(), String> + Send + 'static,
    ) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let (completed, completion) = mpsc::channel();
        let output = path.clone();
        match std::thread::Builder::new()
            .name("escope-png-save".into())
            .spawn(move || {
                let result = save(&output);
                let _ = completed.send(result);
                ctx.request_repaint();
            }) {
            Ok(_) => {
                self.status = SaveStatus::Saving(path.clone());
                self.pending = Some(Pending { path, completion });
            }
            Err(error) => {
                self.status = SaveStatus::Failed {
                    path,
                    error: error.to_string(),
                };
            }
        }
        true
    }

    pub fn poll(&mut self) {
        let Some(pending) = &self.pending else {
            return;
        };
        let result = match pending.completion.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("PNG save worker stopped before returning a result".into())
            }
        };
        let pending = self.pending.take().expect("pending save");
        self.status = match result {
            Ok(()) => SaveStatus::Saved(pending.path),
            Err(error) => SaveStatus::Failed {
                path: pending.path,
                error,
            },
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use escope_host::protocol::{CaptureMeta, PixelFormat};
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    static DIRECTORY_ID: AtomicU64 = AtomicU64::new(0);

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let id = DIRECTORY_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("escope-png-{}-{stamp}-{id}", std::process::id()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn capture() -> Arc<Capture> {
        Arc::new(
            Capture::from_raw(
                CaptureMeta {
                    capture_id: 1,
                    width: 2,
                    height: 1,
                    stride: 6,
                    format: PixelFormat::Rgb888,
                    total_bytes: 6,
                    frame_id: 7,
                },
                &[255, 0, 0, 0, 255, 0],
            )
            .unwrap(),
        )
    }

    fn finish(save: &mut PngSaver) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while save.is_saving() {
            save.poll();
            assert!(Instant::now() < deadline, "save worker did not finish");
            std::thread::yield_now();
        }
    }

    #[test]
    fn slow_save_retains_the_frame_after_disconnect_and_allows_ui_frames() {
        let directory = Directory::new();
        let path = directory.0.join("frame.png");
        let ctx = egui::Context::default();
        let original = capture();
        let retained = Arc::clone(&original);
        let weak = Arc::downgrade(&original);
        let caller = std::thread::current().id();
        let (entered, started) = mpsc::channel();
        let (release, held) = mpsc::channel();
        let mut save = PngSaver::default();
        assert!(save.start_with(path.clone(), ctx.clone(), move |path| {
            assert_ne!(std::thread::current().id(), caller);
            entered.send(()).unwrap();
            held.recv_timeout(Duration::from_secs(5)).unwrap();
            retained.save_png(path).map_err(|error| error.to_string())
        }));
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        // clear_device drops the Inspector's capture. The submitted frame is
        // still owned by the saver, including while its filesystem is stalled.
        drop(original);
        assert!(weak.upgrade().is_some());
        save.poll();
        assert!(save.is_saving());
        assert!(!save.start_with(path.clone(), ctx.clone(), |_| panic!("duplicate save")));
        let mut rendered = ctx.run_ui(Default::default(), |root| {
            egui::CentralPanel::default().show(root, |ui| {
                ui.button("Disconnect").clicked();
            });
        });
        assert!(!rendered.shapes.is_empty());
        rendered.textures_delta.clear();
        release.send(()).unwrap();
        finish(&mut save);
        assert!(matches!(save.status(), SaveStatus::Saved(saved) if saved == &path));
        assert_eq!(&std::fs::read(&path).unwrap()[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn existing_files_are_preserved_and_failed_exports_can_retry() {
        let directory = Directory::new();
        let occupied = directory.0.join("existing.png");
        std::fs::write(&occupied, b"keep this file").unwrap();
        let ctx = egui::Context::default();
        let mut save = PngSaver::default();
        save.start(capture(), occupied.clone(), ctx.clone());
        finish(&mut save);
        assert!(
            matches!(save.status(), SaveStatus::Failed { path, error } if path == &occupied && !error.is_empty())
        );
        assert_eq!(std::fs::read(&occupied).unwrap(), b"keep this file");
        let missing = directory.0.join("missing/frame.png");
        save.start(capture(), missing.clone(), ctx.clone());
        finish(&mut save);
        assert!(matches!(save.status(), SaveStatus::Failed { path, .. } if path == &missing));
        let retry = directory.0.join("retry.png");
        save.start(capture(), retry.clone(), ctx);
        finish(&mut save);
        assert!(matches!(save.status(), SaveStatus::Saved(path) if path == &retry));
        assert!(retry.is_file());
    }
}
