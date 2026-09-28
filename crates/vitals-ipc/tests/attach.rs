//! End-to-end tests over a real local socket.
//!
//! Not against `Decoder` alone: the framing tests in the module cover that.
//! These bind a pipe under a random name, connect the way the CLI does, and
//! assert on what the CLI would receive — because the defect this transport
//! exists to prevent is a client that connects mid-stream and sees a delta it
//! cannot apply.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use interprocess::local_socket::traits::Stream as _;
use vitals_core::fixtures;
use vitals_core::ids::Pid;
use vitals_core::sample::{Frame, FramePayload, FrameSeq};
use vitals_ipc::attach::{AttachClient, AttachServer, ProtocolError};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A name no other test or process is using: pid plus a counter.
fn unique_name() -> String {
    format!(
        "vitals-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    )
}

fn delta(seq: u64, changed: Vec<vitals_core::process::Process>, exited: Vec<Pid>) -> Frame {
    Frame {
        seq: FrameSeq(seq),
        timestamp_ms: seq,
        elapsed_ms: 1_000,
        payload: FramePayload::Delta {
            system: fixtures::system(),
            changed,
            exited,
        },
    }
}

fn process_names(frame: &Frame) -> Vec<String> {
    let FramePayload::Keyframe { processes, .. } = &frame.payload else {
        panic!("expected a keyframe, got {:?}", frame.payload);
    };
    processes.iter().map(|p| p.name.clone()).collect()
}

/// Publishing when nobody is attached is skipped by the host; the server has
/// to be given something after a client appears. This helper waits for the
/// client count then publishes.
fn publish_when_attached(server: &AttachServer, frame: Frame) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !server.has_clients() {
        assert!(Instant::now() < deadline, "client never connected");
        std::thread::sleep(Duration::from_millis(5));
    }
    server.publish(&Arc::new(frame));
}

#[test]
fn hello_reports_the_app_version_and_the_model_version() {
    let name = unique_name();
    let _server = AttachServer::start(&name, "9.9.9".into()).expect("bind");
    let (_client, hello) = AttachClient::connect(&name).expect("connect");
    assert_eq!(hello.status, "ok");
    assert_eq!(hello.version, "9.9.9");
    assert_eq!(hello.model_version, vitals_core::MODEL_VERSION);
}

#[test]
fn a_subscriber_joining_mid_stream_first_receives_a_complete_keyframe_not_the_latest_delta() {
    let name = unique_name();
    let server = AttachServer::start(&name, "t".into()).expect("bind");
    // Drive the server through a keyframe and two deltas *before* anyone
    // connects, so "latest" is a delta and the keyframe is stale.
    server.publish(&Arc::new(fixtures::keyframe(
        1,
        vec![fixtures::process("old.exe", 100, 1.0)],
    )));
    server.publish(&Arc::new(delta(
        2,
        vec![fixtures::process("new.exe", 100, 2.0)],
        vec![Pid(100)],
    )));
    server.publish(&Arc::new(delta(
        3,
        vec![fixtures::process("chrome.exe", 4242, 12.0)],
        vec![],
    )));

    let (client, _) = AttachClient::connect(&name).expect("connect");
    let mut frames = client.subscribe().expect("subscribe");
    let first = frames.next().expect("a frame").expect("parses");

    assert!(first.is_keyframe(), "first frame must be materialised");
    assert_eq!(
        first.seq,
        FrameSeq(3),
        "and current, not the stale keyframe"
    );
    let mut names = process_names(&first);
    names.sort();
    assert_eq!(
        names,
        vec!["chrome.exe", "new.exe"],
        "exits applied before changes: the recycled PID's new owner survives"
    );
}

#[test]
fn frames_published_after_subscribing_arrive_in_order() {
    let name = unique_name();
    let server = Arc::new(AttachServer::start(&name, "t".into()).expect("bind"));
    server.publish(&Arc::new(fixtures::keyframe(1, vec![])));

    let (client, _) = AttachClient::connect(&name).expect("connect");
    let mut frames = client.subscribe().expect("subscribe");
    let first = frames.next().unwrap().unwrap();
    assert_eq!(first.seq, FrameSeq(1));

    server.publish(&Arc::new(delta(2, vec![], vec![])));
    server.publish(&Arc::new(delta(3, vec![], vec![])));
    assert_eq!(frames.next().unwrap().unwrap().seq, FrameSeq(2));
    assert_eq!(frames.next().unwrap().unwrap().seq, FrameSeq(3));
}

#[test]
fn snapshot_returns_one_complete_frame_and_the_server_then_closes_the_connection() {
    let name = unique_name();
    let server = AttachServer::start(&name, "t".into()).expect("bind");
    server.publish(&Arc::new(fixtures::keyframe(
        7,
        vec![fixtures::process("a.exe", 1, 0.0)],
    )));
    let (client, _) = AttachClient::connect(&name).expect("connect");
    let frame = client.snapshot().expect("snapshot");
    assert!(frame.is_keyframe());
    assert_eq!(process_names(&frame), vec!["a.exe"]);
}

#[test]
fn a_client_attaching_to_an_idle_server_asks_for_a_keyframe_and_gets_the_next_one() {
    let name = unique_name();
    let server = Arc::new(AttachServer::start(&name, "t".into()).expect("bind"));
    assert!(!server.wants_keyframe(), "no client, nothing wanted");

    let feeder = Arc::clone(&server);
    let feed = std::thread::spawn(move || {
        publish_when_attached(
            &feeder,
            fixtures::keyframe(1, vec![fixtures::process("first.exe", 5, 1.0)]),
        );
    });

    let (client, _) = AttachClient::connect(&name).expect("connect");
    // Between connect and the first publish the host would read this.
    let deadline = Instant::now() + Duration::from_secs(2);
    while !server.wants_keyframe() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    let frame = client.snapshot().expect("snapshot arrives once published");
    feed.join().unwrap();
    assert_eq!(process_names(&frame), vec!["first.exe"]);
}

#[test]
fn the_client_count_is_zero_before_and_after_a_connection_so_the_host_can_skip_work() {
    let name = unique_name();
    let server = AttachServer::start(&name, "t".into()).expect("bind");
    assert!(!server.has_clients());
    {
        let (_client, _) = AttachClient::connect(&name).expect("connect");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !server.has_clients() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(server.has_clients());
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while server.has_clients() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        !server.has_clients(),
        "a departed client must be counted out"
    );
}

#[test]
fn a_client_attaching_after_the_last_one_left_is_not_served_the_view_frozen_at_that_detach() {
    let name = unique_name();
    let server = Arc::new(AttachServer::start(&name, "t".into()).expect("bind"));

    // First client: the host publishes while it is attached, then it leaves.
    {
        let (client, _) = AttachClient::connect(&name).expect("connect");
        publish_when_attached(
            &server,
            fixtures::keyframe(1, vec![fixtures::process("stale.exe", 5, 1.0)]),
        );
        let frame = client.snapshot().expect("snapshot");
        assert_eq!(process_names(&frame), vec!["stale.exe"]);
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while server.has_clients() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!server.has_clients());

    // Nobody attached: the host skips publishing, so stale.exe has long
    // exited by the time a second client appears. The server must ask for a
    // fresh keyframe rather than hand over the view it froze at the detach.
    let feeder = Arc::clone(&server);
    let feed = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !feeder.wants_keyframe() {
            assert!(
                Instant::now() < deadline,
                "server never asked for a keyframe"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        feeder.publish(&Arc::new(fixtures::keyframe(
            2,
            vec![fixtures::process("fresh.exe", 6, 1.0)],
        )));
    });
    let (client, _) = AttachClient::connect(&name).expect("connect");
    let frame = client.snapshot().expect("snapshot");
    feed.join().unwrap();
    assert_eq!(frame.seq, FrameSeq(2));
    assert_eq!(process_names(&frame), vec!["fresh.exe"]);
}

#[test]
fn a_garbage_request_gets_an_error_reply_and_the_connection_is_closed_not_the_server() {
    use std::io::{BufRead, BufReader, Write};

    let name = unique_name();
    let _server = AttachServer::start(&name, "t".into()).expect("bind");
    let raw = interprocess::local_socket::Stream::connect(
        interprocess::local_socket::ToNsName::to_ns_name::<
            interprocess::local_socket::GenericNamespaced,
        >(name.clone())
        .unwrap(),
    )
    .expect("raw connect");
    let mut reader = BufReader::new(raw);
    reader.get_mut().write_all(b"this is not json\n").unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("\"status\":\"error\""), "{line}");
    line.clear();
    assert_eq!(reader.read_line(&mut line).unwrap(), 0, "server hung up");

    // The server is still alive for the next client.
    let (_ok, hello) = AttachClient::connect(&name).expect("still serving");
    assert_eq!(hello.status, "ok");
}

#[test]
fn connecting_to_a_name_nobody_listens_on_is_an_error_not_a_hang() {
    let started = Instant::now();
    let result = AttachClient::connect(&unique_name());
    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_server_error_line_is_surfaced_with_the_servers_words() {
    // Exercise the client's error path by hand-rolling a server that speaks
    // the protocol's error object.
    use interprocess::local_socket::traits::Listener as _;
    use interprocess::local_socket::{GenericNamespaced, ListenerOptions, ToNsName};
    use std::io::{BufRead, BufReader, Write};

    let name = unique_name();
    let listener = ListenerOptions::new()
        .name(name.clone().to_ns_name::<GenericNamespaced>().unwrap())
        .create_sync()
        .unwrap();
    let fake = std::thread::spawn(move || {
        let conn = listener.accept().unwrap();
        let mut reader = BufReader::new(conn);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap(); // hello
        reader
            .get_mut()
            .write_all(
                format!(
                    "{{\"status\":\"ok\",\"version\":\"x\",\"modelVersion\":{}}}\n",
                    vitals_core::MODEL_VERSION
                )
                .as_bytes(),
            )
            .unwrap();
        line.clear();
        reader.read_line(&mut line).unwrap(); // snapshot
        reader
            .get_mut()
            .write_all(b"{\"status\":\"error\",\"message\":\"paused\"}\n")
            .unwrap();
    });
    let (client, _) = AttachClient::connect(&name).unwrap();
    let error = client.snapshot().expect_err("server said error");
    let inner = error
        .get_ref()
        .and_then(|e| e.downcast_ref::<ProtocolError>())
        .expect("a protocol error");
    assert!(
        matches!(inner, ProtocolError::Server(m) if m == "paused"),
        "{inner}"
    );
    fake.join().unwrap();
}
