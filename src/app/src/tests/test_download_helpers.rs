//! Test file to demonstrate download_helpers usage and verify compilation
//!
//! This file serves as both a test and a demonstration of Oracle-approved patterns.

#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::indexing_slicing)]

mod common;

use common::download_helpers::*;
use std::time::Duration;
use tokio::sync::mpsc;
use lattice::features::download::manager::DownloadEvent;

#[tokio::test]
async fn test_helpers_integrate_correctly() {
    let (tx, rx) = mpsc::unbounded_channel();

    tx.send(DownloadEvent::Started {
        id: "test".to_string(),
    })
    .unwrap();

    tx.send(DownloadEvent::Completed {
        id: "test".to_string(),
    })
    .unwrap();

    let events = collect_download_events(rx, Duration::from_secs(1)).await;

    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], DownloadEvent::Started { .. }));
    assert!(matches!(events[1], DownloadEvent::Completed { .. }));
}

#[tokio::test]
async fn test_progress_verification() {
    let (tx, rx) = mpsc::channel(10);

    tokio::spawn(async move {
        tx.send((100, 10.0)).await.unwrap();
        tx.send((200, 20.0)).await.unwrap();
        tx.send((300, 30.0)).await.unwrap();
    });

    let result = verify_progress_sequence(rx, vec![100, 200, 300], 5).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_poll_until_database_pattern() {
    let mut counter = 0;

    let result = poll_until(
        || {
            counter += 1;
            Box::pin(async move {
                if counter >= 3 {
                    Some("found")
                } else {
                    None
                }
            })
        },
        Duration::from_secs(2),
    )
    .await;

    assert_eq!(result.unwrap(), "found");
}
