#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use super::*;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
struct Flags {
    killed: AtomicBool,
    crashed: AtomicBool,
}

struct FakeProcess(Arc<Flags>);

impl ServerProcess for FakeProcess {
    fn has_exited(&mut self) -> bool {
        self.0.killed.load(Ordering::SeqCst) || self.0.crashed.load(Ordering::SeqCst)
    }

    fn kill(&mut self) {
        self.0.killed.store(true, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct FakeLauncher {
    fail: AtomicBool,
    processes: SyncMutex<Vec<Arc<Flags>>>,
}

impl FakeLauncher {
    fn launches(&self) -> usize {
        self.processes.lock().len()
    }

    fn process(&self, index: usize) -> Arc<Flags> {
        Arc::clone(&self.processes.lock()[index])
    }
}

#[async_trait]
impl ServerLauncher for FakeLauncher {
    async fn launch(&self, _model: &OcrModel) -> Result<RunningServer, OcrError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(OcrError::Unavailable {
                reason: "binary missing".into(),
            });
        }
        let flags = Arc::new(Flags::default());
        let mut processes = self.processes.lock();
        processes.push(Arc::clone(&flags));
        Ok(RunningServer::new(
            format!("http://127.0.0.1:{}", 9000 + processes.len()),
            "key".into(),
            Box::new(FakeProcess(flags)),
        ))
    }
}

fn model(id: &str) -> OcrModel {
    OcrModel::in_dir(id, Path::new("/models"), "OCR:")
}

fn server(launcher: &Arc<FakeLauncher>, idle_ttl: Duration) -> OcrServer {
    OcrServer::build(
        Arc::clone(launcher) as Arc<dyn ServerLauncher>,
        idle_ttl,
        Some(CancellationToken::new()),
    )
}

async fn read_page(server: &OcrServer, model: &OcrModel) -> Result<String, OcrError> {
    server
        .run(model, |address| async move { Ok(address.endpoint) })
        .await
}

#[tokio::test]
async fn consecutive_pages_share_one_process() {
    let launcher = Arc::new(FakeLauncher::default());
    let server = server(&launcher, Duration::from_secs(60));

    let first = read_page(&server, &model("ocr")).await.unwrap();
    let second = read_page(&server, &model("ocr")).await.unwrap();

    assert_eq!(first, second);
    assert_eq!(launcher.launches(), 1);
}

#[tokio::test]
async fn a_launch_failure_keeps_nothing_and_the_next_page_tries_again() {
    let launcher = Arc::new(FakeLauncher::default());
    launcher.fail.store(true, Ordering::SeqCst);
    let server = server(&launcher, Duration::from_secs(60));

    let error = read_page(&server, &model("ocr")).await.unwrap_err();
    assert!(matches!(error, OcrError::Unavailable { .. }));
    assert!(!server.is_running().await);

    launcher.fail.store(false, Ordering::SeqCst);
    read_page(&server, &model("ocr")).await.unwrap();
    assert!(server.is_running().await);
}

#[tokio::test]
async fn an_idle_server_is_stopped_and_its_process_killed() {
    let launcher = Arc::new(FakeLauncher::default());
    let server = server(&launcher, Duration::from_millis(40));

    read_page(&server, &model("ocr")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(250)).await;

    assert!(!server.is_running().await);
    assert!(launcher.process(0).killed.load(Ordering::SeqCst));
}

/// The page clock restarts when a page finishes, so a slow page is not
/// followed by an immediate shutdown, and the reaper never stops a server
/// under a page.
#[tokio::test]
async fn a_slow_page_holds_off_the_idle_stop() {
    let launcher = Arc::new(FakeLauncher::default());
    let server = server(&launcher, Duration::from_millis(80));

    server
        .run(&model("ocr"), |_| async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            Ok::<_, OcrError>(())
        })
        .await
        .unwrap();

    assert!(server.is_running().await);
    assert!(!launcher.process(0).killed.load(Ordering::SeqCst));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!server.is_running().await);
}

#[tokio::test]
async fn a_crashed_server_is_replaced_on_the_next_page() {
    let launcher = Arc::new(FakeLauncher::default());
    let server = server(&launcher, Duration::from_secs(60));

    read_page(&server, &model("ocr")).await.unwrap();
    launcher.process(0).crashed.store(true, Ordering::SeqCst);
    read_page(&server, &model("ocr")).await.unwrap();

    assert_eq!(launcher.launches(), 2);
    assert!(server.is_running().await);
}

#[tokio::test]
async fn a_different_model_gets_a_fresh_server() {
    let launcher = Arc::new(FakeLauncher::default());
    let server = server(&launcher, Duration::from_secs(60));

    read_page(&server, &model("one")).await.unwrap();
    read_page(&server, &model("two")).await.unwrap();

    assert_eq!(launcher.launches(), 2);
    assert!(launcher.process(0).killed.load(Ordering::SeqCst));
    assert!(!launcher.process(1).killed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn shutdown_kills_the_process_even_under_a_page() {
    let launcher = Arc::new(FakeLauncher::default());
    let shutdown = CancellationToken::new();
    let server = Arc::new(OcrServer::build(
        Arc::clone(&launcher) as Arc<dyn ServerLauncher>,
        Duration::from_secs(60),
        Some(shutdown.clone()),
    ));
    let page = {
        let server = Arc::clone(&server);
        tokio::spawn(async move {
            server
                .run(&model("ocr"), |_| async {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    Ok::<_, OcrError>(())
                })
                .await
        })
    };
    while launcher.launches() == 0 {
        tokio::task::yield_now().await;
    }

    shutdown.cancel();
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(launcher.process(0).killed.load(Ordering::SeqCst));
    page.abort();
}

#[tokio::test]
async fn dropping_the_server_kills_its_process() {
    let launcher = Arc::new(FakeLauncher::default());
    let server = server(&launcher, Duration::from_secs(60));
    read_page(&server, &model("ocr")).await.unwrap();

    drop(server);

    assert!(launcher.process(0).killed.load(Ordering::SeqCst));
}

#[test]
fn idle_is_measured_from_the_last_page() {
    let now = Instant::now();
    let ttl = Duration::from_secs(60);
    assert!(!idle_expired(now, now + Duration::from_secs(59), ttl));
    assert!(idle_expired(now, now + ttl, ttl));
    // A clock read before `last_used` (a page finished meanwhile) is not idle.
    assert!(!idle_expired(now + Duration::from_secs(1), now, ttl));
}
