use super::*;

impl InferenceScheduler {
    pub(super) fn queued(&self) -> usize {
        self.state.lock().queue.len()
    }

    /// How long until admission reopens, while a throttle keeps it closed.
    pub(crate) fn reopens_in(&self) -> Option<Duration> {
        let open_at = self.state.lock().congestion.as_ref()?.open_at()?;
        let wait = open_at.saturating_duration_since(Instant::now());
        (!wait.is_zero()).then_some(wait)
    }
}

fn keyed_request(key: &str) -> AdmissionRequest {
    AdmissionRequest {
        priority: InferencePriority::Background,
        tokens: 10,
        cache_key: Some(key.to_owned()),
        rank: None,
    }
}

#[tokio::test]
async fn concurrent_same_key_call_does_not_wipe_active_slots_last_key() -> Result<()> {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::llama_server(3, 100_000),
    ));
    let first = scheduler.admit(keyed_request("course"), None).await?;
    assert_eq!(first.slot(), Some(0));

    // While slot 0 is running, a second request with the same key takes slot 1.
    let second = scheduler.admit(keyed_request("course"), None).await?;
    assert_eq!(second.slot(), Some(1));

    // Slot 0 must still advertise "course" so after both finish, slot 0 retains its affinity.
    {
        let state = scheduler.state.lock();
        assert_eq!(
            state.slots.first().and_then(|s| s.last_key.as_deref()),
            Some("course")
        );
        assert_eq!(
            state.slots.get(1).and_then(|s| s.last_key.as_deref()),
            Some("course")
        );
    }

    // Release slot 1 first, then slot 0.
    drop(second);
    drop(first);

    // Next call returns to slot 0 (first matching affine slot), and clears idle duplicate slot 1.
    let third = scheduler.admit(keyed_request("course"), None).await?;
    assert_eq!(third.slot(), Some(0));
    {
        let state = scheduler.state.lock();
        assert_eq!(
            state.slots.first().and_then(|s| s.last_key.as_deref()),
            Some("course")
        );
        assert_eq!(state.slots.get(1).and_then(|s| s.last_key.as_deref()), None);
    }
    Ok(())
}
