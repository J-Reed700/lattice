use std::error::Error;
use std::time::Duration;

use bytes::Bytes;
use http::{Response, StatusCode};

use super::loopback_client_builder;

const FRAMES: usize = 2_000;
const FRAME: &[u8] = b"data: {}\n\n";

async fn serve_event_stream(listener: tokio::net::TcpListener) {
    let (socket, _) = listener.accept().await.unwrap();
    let mut connection = h2::server::handshake(socket).await.unwrap();
    while let Some(Ok((_request, mut respond))) = connection.accept().await {
        tokio::spawn(async move {
            let response = Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/event-stream")
                .body(())
                .unwrap();
            let mut stream = respond.send_response(response, false).unwrap();
            for _ in 0..FRAMES {
                stream.send_data(Bytes::from_static(FRAME), false).unwrap();
            }
            stream.send_data(Bytes::new(), true).unwrap();
        });
    }
}

fn error_chain(error: &dyn Error) -> String {
    let mut parts = vec![error.to_string()];
    let mut source = error.source();
    while let Some(next) = source {
        parts.push(next.to_string());
        source = next.source();
    }
    parts.join(": ")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reader_that_falls_behind_keeps_the_connection() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(serve_event_stream(listener));

    let client = loopback_client_builder()
        .http2_prior_knowledge()
        .build()
        .unwrap();

    let read = async {
        let mut response = client
            .get(format!("http://127.0.0.1:{port}/"))
            .send()
            .await
            .map_err(|error| error_chain(&error))?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let mut total = 0;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| error_chain(&error))?
        {
            total += chunk.len();
        }
        Ok::<usize, String>(total)
    };

    let total = tokio::time::timeout(Duration::from_secs(30), read)
        .await
        .expect("the read timed out")
        .unwrap_or_else(|chain| panic!("the body failed to read: {chain}"));
    assert_eq!(total, FRAMES * FRAME.len());
}
