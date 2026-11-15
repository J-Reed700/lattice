use super::*;
use std::io::Read;
use std::net::{Shutdown, TcpListener};

#[derive(Debug)]
struct Layer {
    message: &'static str,
    cause: Option<Box<Layer>>,
}

impl Display for Layer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}

impl Error for Layer {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause
            .as_deref()
            .map(|cause| cause as &(dyn Error + 'static))
    }
}

fn layer(message: &'static str, cause: Option<Layer>) -> Layer {
    Layer {
        message,
        cause: cause.map(Box::new),
    }
}

#[test]
fn every_cause_is_shown_beneath_the_message() {
    let top = layer(
        "error sending request for url (http://host.invalid/)",
        Some(layer(
            "client error (Connect)",
            Some(layer("connection reset", None)),
        )),
    );
    assert_eq!(
        WithCauses(&top).to_string(),
        "error sending request for url (http://host.invalid/): client error (Connect): connection reset"
    );
}

#[test]
fn a_cause_already_shown_is_left_out() {
    let top = layer("read failed: disk full", Some(layer("disk full", None)));
    assert_eq!(WithCauses(&top).to_string(), "read failed: disk full");
}

#[test]
fn an_error_without_causes_is_its_message() {
    let only = layer("nothing beneath", None);
    assert_eq!(WithCauses(&only).to_string(), "nothing beneath");
}

#[tokio::test]
async fn a_dropped_connection_shows_why_the_request_failed() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut seen = Vec::new();
        let mut buffer = [0u8; 256];
        while !seen.windows(4).any(|window| window == b"\r\n\r\n") {
            match stream.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => seen.extend_from_slice(&buffer[..count]),
            }
        }
        let _ = stream.shutdown(Shutdown::Both);
    });

    let result = reqwest::Client::new()
        .get(format!("http://{addr}/"))
        .send()
        .await;
    server.join().unwrap();

    let Err(error) = result else {
        panic!("the dropped connection must fail the request");
    };
    let shown = WithCauses(&error).to_string();
    let top = error.to_string();
    assert!(shown.starts_with(&top), "{shown}");
    assert!(shown.len() > top.len(), "{shown}");
}
