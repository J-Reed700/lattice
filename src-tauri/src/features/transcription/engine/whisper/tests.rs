use super::decode::{argmax, compression_ratio};
use super::*;
use candle_transformers::models::whisper;

#[test]
fn only_one_idle_unload_timer_can_be_active() {
    let active = Arc::new(AtomicBool::new(false));
    let barrier = Arc::new(std::sync::Barrier::new(16));
    let workers: Vec<_> = (0..16)
        .map(|_| {
            let active = Arc::clone(&active);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                claim_idle_timer(&active)
            })
        })
        .collect();
    let claims = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .filter(|claimed| *claimed)
        .count();

    assert_eq!(claims, 1);
    active.store(false, Ordering::Release);
    assert!(claim_idle_timer(&active));
}

#[test]
fn idle_deadline_recheck_keeps_a_recently_used_model_loaded() {
    let last_used = Instant::now();

    assert!(!idle_expired(
        last_used,
        last_used + TRANSCRIPTION_IDLE_TTL - Duration::from_millis(1),
        TRANSCRIPTION_IDLE_TTL,
    ));
    assert!(idle_expired(
        last_used,
        last_used + TRANSCRIPTION_IDLE_TTL,
        TRANSCRIPTION_IDLE_TTL,
    ));
}

#[tokio::test]
async fn blocking_inference_keeps_its_permit_if_its_async_caller_is_dropped() {
    let permits = Arc::new(tokio::sync::Semaphore::new(1));
    let permit = Arc::clone(&permits).acquire_owned().await.unwrap();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let child = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let _ = started_tx.send(());
        let _ = release_rx.recv();
    });

    started_rx.await.unwrap();
    child.abort();
    assert!(Arc::clone(&permits).try_acquire_owned().is_err());
    release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Ok(permit) = Arc::clone(&permits).try_acquire_owned() {
                drop(permit);
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn stale_idle_timer_rechecks_use_after_waiting_for_inference_gate() {
    let idle_ttl = Duration::from_millis(100);
    let permits = Arc::new(tokio::sync::Semaphore::new(1));
    let inference_permit = Arc::clone(&permits).acquire_owned().await.unwrap();
    let last_used = Arc::new(parking_lot::Mutex::new(
        Instant::now() - idle_ttl + Duration::from_millis(30),
    ));
    let resident_model = Arc::new(parking_lot::Mutex::new(Some(())));
    let cancel = tokio_util::sync::CancellationToken::new();
    let timer = tokio::spawn(wait_for_idle_permit(
        Arc::clone(&permits),
        Arc::clone(&last_used),
        cancel,
        idle_ttl,
    ));

    // The timer's first deadline passes while inference still owns the
    // gate. Inference uses the model again before releasing that gate.
    tokio::time::sleep(Duration::from_millis(50)).await;
    *last_used.lock() = Instant::now();
    drop(inference_permit);

    tokio::pin!(timer);
    assert!(tokio::time::timeout(Duration::from_millis(20), &mut timer)
        .await
        .is_err());
    assert!(resident_model.lock().is_some());
    let idle_permit = tokio::time::timeout(Duration::from_secs(1), &mut timer)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(resident_model.lock().take().is_some());
    drop(idle_permit);
}

#[test]
fn compression_ratio_flags_repetition_loops() {
    let healthy = "The quarterly numbers were up eleven percent across enterprise renewals.";
    let looping = "thank you thank you thank you thank you thank you thank you thank you";

    assert!(compression_ratio(healthy) < whisper::COMPRESSION_RATIO_THRESHOLD);
    assert!(compression_ratio(looping) > whisper::COMPRESSION_RATIO_THRESHOLD);
}

#[test]
fn argmax_picks_the_largest_value() {
    assert_eq!(argmax(&[0.1, 0.9, 0.4]).unwrap(), 1);
    assert!(argmax(&[]).is_err());
}
