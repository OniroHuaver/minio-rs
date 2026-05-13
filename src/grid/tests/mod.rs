//! Grid module tests.

pub mod benchmark;
pub mod fault;
pub mod harness;

use std::sync::Arc;
use std::time::Duration;

use crate::grid::connection::auth_validate_token;
use crate::grid::connection::Connection;
use crate::grid::error::GridError;
use crate::grid::handler::{single_handler_fn, HandlerRegistry};
use crate::grid::manager::Manager;
use crate::grid::message::{Flags, Message, Op, HANDLER_INVALID};
use crate::grid::msg_types::{
    Bytes, ConnectReq, ConnectResp, MSS, MuxConnectError, PongMsg, TestRequest, TestResponse,
};
use crate::grid::ConnectionState;

// ── Handler constants ──────────────────────────────────────────

const HANDLER_ECHO: u8 = 100;
const HANDLER_ERROR: u8 = 101;
const HANDLER_SLOW: u8 = 102;

// ── Simulated loopback pair ────────────────────────────────────

/// Build a simulated connected pair via out_queue → dispatch piping.
///
/// Both sides share the same handler registry so conn_a's requests
/// are dispatched by conn_b using the same registered handlers.
async fn simulated_pair(
    handlers: Arc<HandlerRegistry>,
) -> (Connection, Connection) {
    let conn_a = Connection::new(
        [1u8; 16],
        "node1:9000".into(),
        "node2:9000".into(),
        true,
        "token".into(),
        auth_validate_token("token".into()),
        handlers.clone(),
    );
    let conn_b = Connection::new(
        [2u8; 16],
        "node2:9000".into(),
        "node1:9000".into(),
        false,
        "token".into(),
        auth_validate_token("token".into()),
        handlers,
    );

    conn_a.set_state(ConnectionState::Connected);
    conn_b.set_state(ConnectionState::Connected);

    tokio::spawn({
        let conn_a = conn_a.clone();
        let conn_b = conn_b.clone();
        let mut a_rx = conn_a.take_out_rx().expect("out_rx available");
        let mut b_rx = conn_b.take_out_rx().expect("out_rx available");
        async move {
            loop {
                tokio::select! {
                    Some(msg) = a_rx.recv() => { conn_b.dispatch(msg).await; }
                    Some(msg) = b_rx.recv() => { conn_a.dispatch(msg).await; }
                    else => break,
                }
            }
        }
    });

    tokio::task::yield_now().await;
    (conn_a, conn_b)
}

// ── Message encode / decode ────────────────────────────────────

#[test]
fn test_message_roundtrip() {
    let mut msg = Message::new(Op::Request, 42);
    msg.mux_id = 12345;
    msg.seq = 7;
    msg.deadline_ms = 60000;
    msg.flags = Flags::CRCXXH3;
    msg.payload = Some(b"hello grid".to_vec());

    let encoded = msg.encode().expect("encode");
    let decoded = Message::decode(&encoded).expect("decode");

    assert_eq!(decoded.mux_id, msg.mux_id);
    assert_eq!(decoded.seq, msg.seq);
    assert_eq!(decoded.deadline_ms, msg.deadline_ms);
    assert_eq!(decoded.handler, msg.handler);
    assert_eq!(decoded.op, Op::Request);
    assert_eq!(decoded.flags, Flags::CRCXXH3);
    assert_eq!(decoded.payload.as_deref(), Some(b"hello grid".as_ref()));
}

#[test]
fn test_message_empty_payload() {
    let mut msg = Message::new(Op::Response, 1);
    msg.payload = Some(vec![]);
    msg.set_zero_payload_flag();
    assert!(msg.flags.contains(Flags::PAYLOAD_IS_ZERO));

    let encoded = msg.encode().expect("encode");
    let decoded = Message::decode(&encoded).expect("decode");
    assert!(decoded.flags.contains(Flags::PAYLOAD_IS_ZERO));
}

#[test]
fn test_message_none_payload() {
    let msg = Message::new(Op::Ping, 0);
    let encoded = msg.encode().expect("encode");
    let decoded = Message::decode(&encoded).expect("decode");
    assert_eq!(decoded.op, Op::Ping);
    assert!(decoded.payload.is_none());
}

#[test]
fn test_message_flag_helpers() {
    let mut msg = Message::new(Op::Request, 7);
    msg.flags.insert(Flags::PAYLOAD_IS_ERR);
    msg.payload = Some(b"error".to_vec());
    assert!(msg.is_error());
    assert!(!msg.is_response());
    assert!(msg.is_request());
}

#[test]
fn test_message_large_payload() {
    let payload = vec![0xABu8; 1024 * 1024];
    let mut msg = Message::new(Op::Request, 1);
    msg.payload = Some(payload.clone());
    let encoded = msg.encode().unwrap();
    let decoded = Message::decode(&encoded).unwrap();
    assert_eq!(decoded.payload.as_deref(), Some(payload.as_slice()));
}

// ── Op conversion ──────────────────────────────────────────────

#[test]
fn test_op_try_from_all_valid() {
    for val in 0u8..=17u8 {
        assert!(Op::try_from(val).is_ok(), "Op value {} should be valid", val);
    }
}

#[test]
fn test_op_try_from_invalid() {
    assert_eq!(Op::try_from(0).unwrap(), Op::Invalid);
    assert!(Op::try_from(18).is_err());
    assert!(Op::try_from(255).is_err());
}

// ── msg_types roundtrip ───────────────────────────────────────

#[test]
fn test_connect_req_roundtrip() {
    let req = ConnectReq {
        id: [1; 16],
        host: "node1:9000".into(),
        time: 1715000000.0,
        token: "secret".into(),
        nonce: 1,
    };
    let data = rmp_serde::to_vec(&req).unwrap();
    let decoded: ConnectReq = rmp_serde::from_slice(&data).unwrap();
    assert_eq!(decoded.host, "node1:9000");
    assert_eq!(decoded.token, "secret");
}

#[test]
fn test_connect_resp_rejected() {
    let resp = ConnectResp {
        id: [0; 16],
        accepted: false,
        rejected_reason: "unauthorized".into(),
    };
    let data = rmp_serde::to_vec(&resp).unwrap();
    let decoded: ConnectResp = rmp_serde::from_slice(&data).unwrap();
    assert!(!decoded.accepted);
    assert_eq!(decoded.rejected_reason, "unauthorized");
}

#[tokio::test]
async fn test_handle_connect_accepts_valid_token() {
    let handlers = Arc::new(HandlerRegistry::new());
    let conn = Connection::new(
        [9u8; 16],
        "node-b:9000".into(),
        "node-a:9000".into(),
        false,
        "secret".into(),
        auth_validate_token("secret".into()),
        handlers,
    );
    conn.set_state(ConnectionState::Connecting);
    let req = ConnectReq {
        id: [1u8; 16],
        host: "node-a:9000".into(),
        time: 0.0,
        token: "secret".into(),
        nonce: 100,
    };
    let mut msg = Message::new(Op::Connect, HANDLER_INVALID);
    msg.mux_id = 42;
    msg.payload = Some(rmp_serde::to_vec(&req).unwrap());
    msg.set_zero_payload_flag();
    conn.dispatch(msg).await;
    assert_eq!(conn.state().await, ConnectionState::Connected);
}

#[tokio::test]
async fn test_handle_connect_rejects_bad_token() {
    let handlers = Arc::new(HandlerRegistry::new());
    let conn = Connection::new(
        [9u8; 16],
        "node-b:9000".into(),
        "node-a:9000".into(),
        false,
        "secret".into(),
        auth_validate_token("secret".into()),
        handlers,
    );
    conn.set_state(ConnectionState::Connecting);
    let req = ConnectReq {
        id: [1u8; 16],
        host: "node-a:9000".into(),
        time: 0.0,
        token: "wrong".into(),
        nonce: 101,
    };
    let mut msg = Message::new(Op::Connect, HANDLER_INVALID);
    msg.mux_id = 1;
    msg.payload = Some(rmp_serde::to_vec(&req).unwrap());
    msg.set_zero_payload_flag();
    conn.dispatch(msg).await;
    assert_eq!(conn.state().await, ConnectionState::ConnectionError);
}

#[tokio::test]
async fn test_handle_connect_response_accepts() {
    let handlers = Arc::new(HandlerRegistry::new());
    let conn = Connection::new(
        [5u8; 16],
        "node-a:9000".into(),
        "node-b:9000".into(),
        true,
        "x".into(),
        auth_validate_token("x".into()),
        handlers,
    );
    conn.set_state(ConnectionState::Connecting);
    let resp = ConnectResp {
        id: [7u8; 16],
        accepted: true,
        rejected_reason: String::new(),
    };
    let mut msg = Message::new(Op::ConnectResponse, HANDLER_INVALID);
    msg.payload = Some(rmp_serde::to_vec(&resp).unwrap());
    msg.set_zero_payload_flag();
    conn.dispatch(msg).await;
    assert_eq!(conn.state().await, ConnectionState::Connected);
}

#[tokio::test]
async fn test_handle_connect_response_rejects() {
    let handlers = Arc::new(HandlerRegistry::new());
    let conn = Connection::new(
        [5u8; 16],
        "node-a:9000".into(),
        "node-b:9000".into(),
        true,
        "x".into(),
        auth_validate_token("x".into()),
        handlers,
    );
    conn.set_state(ConnectionState::Connecting);
    let resp = ConnectResp {
        id: [7u8; 16],
        accepted: false,
        rejected_reason: "no".into(),
    };
    let mut msg = Message::new(Op::ConnectResponse, HANDLER_INVALID);
    msg.payload = Some(rmp_serde::to_vec(&resp).unwrap());
    msg.set_zero_payload_flag();
    conn.dispatch(msg).await;
    assert_eq!(conn.state().await, ConnectionState::ConnectionError);
}

#[test]
fn test_pong_msg_with_error() {
    let pong = PongMsg {
        not_found: true,
        err: Some("peer not found".into()),
        t: 0.0,
    };
    let data = rmp_serde::to_vec(&pong).unwrap();
    let decoded: PongMsg = rmp_serde::from_slice(&data).unwrap();
    assert!(decoded.not_found);
    assert_eq!(decoded.err.as_deref(), Some("peer not found"));
}

#[test]
fn test_mux_connect_error_roundtrip() {
    let err = MuxConnectError {
        error: "timeout".into(),
    };
    let data = rmp_serde::to_vec(&err).unwrap();
    let decoded: MuxConnectError = rmp_serde::from_slice(&data).unwrap();
    assert_eq!(decoded.error, "timeout");
}

#[test]
fn test_mss_roundtrip() {
    let mss = MSS::with_entries([("vol".into(), "bucket".into()), ("path".into(), "obj1".into())]);
    let data = rmp_serde::to_vec(&mss).unwrap();
    let decoded: MSS = rmp_serde::from_slice(&data).unwrap();
    assert_eq!(decoded.get("vol").map(|s| s.as_str()), Some("bucket"));
    assert_eq!(decoded.get("path").map(|s| s.as_str()), Some("obj1"));
}

#[test]
fn test_bytes_roundtrip() {
    let b = Bytes::new(vec![1, 2, 3, 4, 5]);
    let data = rmp_serde::to_vec(&b).unwrap();
    let decoded: Bytes = rmp_serde::from_slice(&data).unwrap();
    assert_eq!(&*decoded, &[1, 2, 3, 4, 5]);
}

#[test]
fn test_test_request_roundtrip() {
    let req = TestRequest { org_num: 42, org_string: "hello".into() };
    let data = rmp_serde::to_vec(&req).unwrap();
    let decoded: TestRequest = rmp_serde::from_slice(&data).unwrap();
    assert_eq!(decoded.org_num, 42);
    assert_eq!(decoded.org_string, "hello");
}

#[test]
fn test_test_response_roundtrip() {
    let resp = TestResponse {
        org_num: 7,
        org_string: "world".into(),
        embedded: Some(TestRequest { org_num: 99, org_string: "nested".into() }),
    };
    let data = rmp_serde::to_vec(&resp).unwrap();
    let decoded: TestResponse = rmp_serde::from_slice(&data).unwrap();
    assert_eq!(decoded.org_num, 7);
    assert!(decoded.embedded.is_some());
    assert_eq!(decoded.embedded.unwrap().org_num, 99);
}

#[tokio::test]
async fn test_request_not_connected() {
    let handlers = Arc::new(HandlerRegistry::new());
    let conn = Connection::new(
        [0u8; 16],
        "a:1".into(),
        "b:1".into(),
        true,
        "t".into(),
        auth_validate_token("t".into()),
        handlers,
    );
    let r = conn.request(HANDLER_ECHO, Some(vec![]), None).await;
    assert!(matches!(r, Err(GridError::NotConnected)));
}

#[tokio::test]
async fn test_mux_one_shot_roundtrip() {
    let handlers = Arc::new(HandlerRegistry::new());
    handlers.singles.write().await.insert(
        HANDLER_ECHO,
        single_handler_fn(|payload: Vec<u8>| async move { Ok(payload) }),
    );
    let (conn_a, _conn_b) = simulated_pair(handlers).await;
    let out = conn_a
        .mux_request(HANDLER_ECHO, Some(b"muxdata".to_vec()), None)
        .await
        .expect("mux");
    assert_eq!(out, b"muxdata");
}

#[tokio::test]
async fn test_connect_nonce_replay_rejected() {
    let handlers = Arc::new(HandlerRegistry::new());
    let conn = Connection::new(
        [9u8; 16],
        "node-b:9000".into(),
        "node-a:9000".into(),
        false,
        "secret".into(),
        auth_validate_token("secret".into()),
        handlers,
    );
    conn.set_state(ConnectionState::Connecting);
    let req = ConnectReq {
        id: [1u8; 16],
        host: "node-a:9000".into(),
        time: 0.0,
        token: "secret".into(),
        nonce: 200,
    };
    let mut m1 = Message::new(Op::Connect, HANDLER_INVALID);
    m1.mux_id = 1;
    m1.payload = Some(rmp_serde::to_vec(&req).unwrap());
    m1.set_zero_payload_flag();
    conn.dispatch(m1).await;
    assert_eq!(conn.state().await, ConnectionState::Connected);

    conn.set_state(ConnectionState::ConnectionError);
    conn.set_state(ConnectionState::Connecting);

    let mut m2 = Message::new(Op::Connect, HANDLER_INVALID);
    m2.mux_id = 2;
    m2.payload = Some(rmp_serde::to_vec(&req).unwrap());
    m2.set_zero_payload_flag();
    conn.dispatch(m2).await;
    assert_eq!(conn.state().await, ConnectionState::ConnectionError);
}

#[tokio::test]
async fn test_enter_reconnecting() {
    let handlers = Arc::new(HandlerRegistry::new());
    let conn = Connection::new(
        [1u8; 16],
        "a".into(),
        "b".into(),
        true,
        "x".into(),
        auth_validate_token("x".into()),
        handlers,
    );
    conn.set_state(ConnectionState::ConnectionError);
    conn.enter_reconnecting().await.expect("enter reconnecting");
    assert_eq!(conn.state().await, ConnectionState::Reconnecting);
    assert!(conn.enter_reconnecting().await.is_err());
}

#[tokio::test]
async fn test_manager_replace_connection() {
    let mgr = Manager::new("local:1".into(), "tok".into());
    let c1 = mgr.connection("peer:1").await;
    let p1 = std::sync::Arc::as_ptr(&c1.inner);
    let c2 = mgr.replace_connection("peer:1").await;
    let p2 = std::sync::Arc::as_ptr(&c2.inner);
    assert_ne!(p1, p2);
}

// ── Single round-trip tests ───────────────────────────────────

#[tokio::test]
async fn test_single_roundtrip_echo() {
    let handlers = Arc::new(HandlerRegistry::new());
    handlers.singles.write().await.insert(
        HANDLER_ECHO,
        single_handler_fn(|payload: Vec<u8>| async move { Ok(payload) }),
    );

    let (conn_a, _conn_b) = simulated_pair(handlers).await;
    let response = conn_a
        .request(HANDLER_ECHO, Some(b"hello".to_vec()), None)
        .await
        .expect("request should succeed");
    assert_eq!(response, b"hello");
}

#[tokio::test]
async fn test_single_roundtrip_error() {
    let handlers = Arc::new(HandlerRegistry::new());
    handlers.singles.write().await.insert(
        HANDLER_ERROR,
        single_handler_fn(|_payload: Vec<u8>| async move {
            Err(crate::grid::RemoteErr { msg: "intentional error".to_string() })
        }),
    );

    let (conn_a, _conn_b) = simulated_pair(handlers).await;
    match conn_a.request(HANDLER_ERROR, Some(vec![]), None).await {
        Err(GridError::Remote(msg)) => assert!(msg.contains("intentional error")),
        other => panic!("expected GridError::Remote, got {:?}", other),
    }
}

#[tokio::test]
async fn test_handler_not_found() {
    let handlers = Arc::new(HandlerRegistry::new());
    let (conn_a, _conn_b) = simulated_pair(handlers).await;
    match conn_a.request(HANDLER_ECHO, Some(vec![]), None).await {
        Err(GridError::Remote(msg)) => assert!(msg.contains("handler not found")),
        other => panic!("expected GridError::Remote, got {:?}", other),
    }
}

#[tokio::test]
async fn test_request_timeout() {
    let handlers = Arc::new(HandlerRegistry::new());
    handlers.singles.write().await.insert(
        HANDLER_SLOW,
        single_handler_fn(|_payload: Vec<u8>| async move {
            tokio::time::sleep(Duration::from_secs(10)).await;
            Ok(vec![])
        }),
    );

    let (conn_a, _conn_b) = simulated_pair(handlers).await;
    let result = conn_a
        .request(HANDLER_SLOW, Some(vec![]), Some(Duration::from_millis(50)))
        .await;
    assert!(matches!(result, Err(GridError::DeadlineExceeded)));
}

#[tokio::test]
async fn test_large_payload_roundtrip() {
    let handlers = Arc::new(HandlerRegistry::new());
    handlers.singles.write().await.insert(
        HANDLER_ECHO,
        single_handler_fn(|payload: Vec<u8>| async move { Ok(payload) }),
    );

    let (conn_a, _conn_b) = simulated_pair(handlers).await;
    let payload = vec![0xCDu8; 1024 * 1024];
    let response = conn_a
        .request(HANDLER_ECHO, Some(payload.clone()), None)
        .await
        .unwrap();
    assert_eq!(response.len(), 1024 * 1024);
    assert_eq!(response, payload);
}

#[tokio::test]
async fn test_concurrent_requests() {
    let handlers = Arc::new(HandlerRegistry::new());
    handlers.singles.write().await.insert(
        HANDLER_ECHO,
        single_handler_fn(|payload: Vec<u8>| async move { Ok(payload) }),
    );

    let (conn_a, _conn_b) = simulated_pair(handlers).await;
    let mut handles = vec![];
    for i in 0..20 {
        let conn = conn_a.clone();
        handles.push(tokio::spawn(async move {
            let payload = format!("msg-{}", i).into_bytes();
            let response = conn
                .request(HANDLER_ECHO, Some(payload.clone()), None)
                .await
                .unwrap();
            assert_eq!(response, payload);
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
}

// ── Connection topology ────────────────────────────────────────

#[test]
fn test_should_connect_symmetry() {
    let a = "node1:9000";
    let b = "node2:9000";
    assert_ne!(
        Connection::should_connect(a, b),
        Connection::should_connect(b, a)
    );
}

#[test]
fn test_should_connect_idempotent() {
    let a = "host-a:9000";
    let b = "host-b:9000";
    assert_eq!(
        Connection::should_connect(a, b),
        Connection::should_connect(a, b)
    );
}

#[test]
fn test_should_connect_many_hosts() {
    let hosts: Vec<String> = (0..10)
        .map(|i| format!("192.168.1.{}:9000", i + 1))
        .collect();
    let mut counts = std::collections::HashMap::new();
    for i in 0..hosts.len() {
        for j in 0..hosts.len() {
            if i == j {
                continue;
            }
            if Connection::should_connect(&hosts[i], &hosts[j]) {
                *counts.entry(i).or_insert(0usize) += 1;
            }
        }
    }
    for (_, count) in &counts {
        assert!(*count > 0, "host should connect to at least one other");
    }
}

// ── Edge cases ─────────────────────────────────────────────────

#[tokio::test]
async fn test_dispatch_unknown_op_no_panic() {
    let handlers = Arc::new(HandlerRegistry::new());
    let conn = Connection::new(
        [0u8; 16],
        "local".into(),
        "remote".into(),
        true,
        "token".into(),
        auth_validate_token("token".into()),
        handlers,
    );
    conn.set_state(ConnectionState::Connected);
    let msg = Message::default();
    conn.dispatch(msg).await;
}

#[tokio::test]
async fn test_response_orphan_no_panic() {
    let handlers = Arc::new(HandlerRegistry::new());
    let conn = Connection::new(
        [0u8; 16],
        "local".into(),
        "remote".into(),
        true,
        "token".into(),
        auth_validate_token("token".into()),
        handlers,
    );
    conn.set_state(ConnectionState::Connected);
    let mut msg = Message::new(Op::Response, 1);
    msg.mux_id = 99999;
    conn.dispatch(msg).await;
}
