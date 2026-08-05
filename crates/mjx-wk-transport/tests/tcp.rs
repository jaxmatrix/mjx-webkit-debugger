//! TcpTransport integration tests against a local glib-framing mock peer.
//!
//! Helpers below sit outside `#[test]` bodies, so clippy's test exemption for
//! `expect_used` does not cover them — allow here rather than hide real
//! assertion failures behind silent `let _ =`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::Duration;

use mjx_wk_transport::Transport;
use mjx_wk_transport::discovery::{SocketEvent, TargetKey, decode_messages, encode_message};
use mjx_wk_transport::{TcpInspectorServer, TransportError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

async fn read_event(
    stream: &mut TcpStream,
    buffer: &mut Vec<u8>,
    pending: &mut std::collections::VecDeque<SocketEvent>,
) -> SocketEvent {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(event) = pending.pop_front() {
            return event;
        }
        let decoded = decode_messages(buffer).expect("decode");
        if !decoded.is_empty() {
            pending.extend(decoded);
            continue;
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(!remaining.is_zero(), "timed out waiting for socket event");
        let mut chunk = [0u8; 65536];
        let n = tokio::time::timeout(remaining, stream.read(&mut chunk))
            .await
            .expect("read timeout")
            .expect("read");
        assert_ne!(n, 0, "peer closed before sending an event");
        buffer.extend_from_slice(&chunk[..n]);
    }
}

async fn write_event(stream: &mut TcpStream, event: &SocketEvent) {
    let framed = encode_message(event).expect("encode");
    stream.write_all(&framed).await.expect("write");
    stream.flush().await.expect("flush");
}

/// Accept one client, complete the glib handshake, then run `after`.
async fn serve_after_handshake<F, Fut>(listener: TcpListener, after: F)
where
    F: FnOnce(TcpStream, Vec<u8>, std::collections::VecDeque<SocketEvent>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let (mut stream, _) = listener.accept().await.expect("accept");
    let mut buffer = Vec::new();
    let mut pending = std::collections::VecDeque::new();
    let setup_client = read_event(&mut stream, &mut buffer, &mut pending).await;
    assert!(
        matches!(setup_client, SocketEvent::SetupInspectorClient { .. }),
        "expected SetupInspectorClient, got {setup_client:?}"
    );
    write_event(
        &mut stream,
        &SocketEvent::DidSetupInspectorClient {
            backend_commands: Vec::new(),
        },
    )
    .await;
    let setup = read_event(&mut stream, &mut buffer, &mut pending).await;
    assert!(
        matches!(
            setup,
            SocketEvent::Setup {
                connection_id: 1,
                target_id: 2,
            }
        ),
        "expected Setup(1, 2), got {setup:?}"
    );
    after(stream, buffer, pending).await;
}

#[tokio::test]
async fn a_refused_connection_names_the_endpoint() {
    // Port 1 is privileged and almost never listening for us.
    let endpoint = "127.0.0.1:1";
    let server = TcpInspectorServer::new(endpoint);
    let err = server
        .attach(&TargetKey::from_ids(1, 2))
        .await
        .expect_err("connect should fail");
    match err {
        TransportError::Connect {
            endpoint: ref named,
            ..
        } => assert_eq!(named, endpoint),
        other => panic!("expected Connect, got {other:?}"),
    }
    assert!(err.to_string().contains(endpoint), "{err}");
}

#[tokio::test]
async fn send_wraps_a_frame_as_send_message_to_backend() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let (tx, rx) = oneshot::channel::<String>();

    tokio::spawn(async move {
        serve_after_handshake(
            listener,
            move |mut stream, mut buffer, mut pending| async move {
                let event = read_event(&mut stream, &mut buffer, &mut pending).await;
                let SocketEvent::SendMessageToBackend { message, .. } = event else {
                    panic!("expected SendMessageToBackend, got {event:?}");
                };
                let _ = tx.send(message);
            },
        )
        .await;
    });

    let mut transport = TcpInspectorServer::new(addr)
        .attach(&TargetKey::from_ids(1, 2))
        .await
        .expect("attach");
    transport
        .send(r#"{"id":1,"method":"Runtime.evaluate"}"#.into())
        .await
        .expect("send");
    let seen = tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .expect("oneshot timeout")
        .expect("message");
    assert_eq!(seen, r#"{"id":1,"method":"Runtime.evaluate"}"#);
    transport.close().await.ok();
}

#[tokio::test]
async fn a_clean_peer_close_reads_as_none() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    tokio::spawn(async move {
        serve_after_handshake(listener, |stream, _buffer, _pending| async move {
            // Drop the stream → EOF for the client.
            drop(stream);
        })
        .await;
    });

    let mut transport = TcpInspectorServer::new(addr)
        .attach(&TargetKey::from_ids(1, 2))
        .await
        .expect("attach");
    let got = transport.recv().await;
    assert!(got.is_none(), "expected clean close, got {got:?}");
}

#[tokio::test]
async fn a_five_megabyte_frame_arriving_in_chunks_is_delivered_once() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let payload = "x".repeat(5 * 1024 * 1024);

    tokio::spawn({
        let payload = payload.clone();
        async move {
            serve_after_handshake(listener, move |mut stream, _buffer, _pending| async move {
                let framed = encode_message(&SocketEvent::SendMessageToFrontend {
                    connection_id: 1,
                    target_id: 2,
                    message: payload,
                })
                .expect("encode");
                // Many small writes — the case that breaks a one-read parser.
                for chunk in framed.chunks(64) {
                    stream.write_all(chunk).await.expect("write chunk");
                    stream.flush().await.expect("flush chunk");
                }
            })
            .await;
        }
    });

    let mut transport = TcpInspectorServer::new(addr)
        .attach(&TargetKey::from_ids(1, 2))
        .await
        .expect("attach");
    let frame = transport.recv().await.expect("a frame").expect("frame ok");
    assert_eq!(frame.len(), 5 * 1024 * 1024);
    assert!(frame.bytes().all(|b| b == b'x'));
    transport.close().await.ok();
}

#[tokio::test]
async fn close_sends_frontend_did_close_before_dropping_the_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let (tx, rx) = oneshot::channel::<SocketEvent>();

    tokio::spawn(async move {
        serve_after_handshake(
            listener,
            move |mut stream, mut buffer, mut pending| async move {
                let event = read_event(&mut stream, &mut buffer, &mut pending).await;
                let _ = tx.send(event);
                // Keep the socket open briefly so the client's write lands cleanly.
                tokio::time::sleep(Duration::from_millis(50)).await;
            },
        )
        .await;
    });

    let mut transport = TcpInspectorServer::new(addr)
        .attach(&TargetKey::from_ids(1, 2))
        .await
        .expect("attach");
    transport.close().await.expect("close");
    // Idempotent.
    transport.close().await.expect("close again");

    let event = tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .expect("oneshot timeout")
        .expect("event");
    assert_eq!(
        event,
        SocketEvent::FrontendDidClose {
            connection_id: 1,
            target_id: 2,
        }
    );
}
