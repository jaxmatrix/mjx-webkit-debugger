//! Framing and discovery integration tests for the glib SocketConnection protocol.

#![cfg(unix)]

use mjx_wk_protocol::TargetType;
use mjx_wk_transport::TransportOrigin;
use mjx_wk_transport::discovery::{
    SocketEvent, SocketTarget, TargetKey, decode_messages, descriptors_from_target_list,
    encode_message,
};

#[test]
fn set_target_list_with_three_targets_yields_three_descriptors_in_order() {
    let event = SocketEvent::SetTargetList {
        connection_id: 1,
        target_list: vec![
            SocketTarget {
                target_id: 2,
                name: "one".into(),
                url: "http://one/".into(),
                kind: "WebPage".into(),
            },
            SocketTarget {
                target_id: 3,
                name: "two".into(),
                url: "http://two/".into(),
                kind: "WebPage".into(),
            },
            SocketTarget {
                target_id: 4,
                name: "three".into(),
                url: "http://three/".into(),
                kind: "WebPage".into(),
            },
        ],
    };
    let mut buf = encode_message(&event).expect("encode");
    let decoded = decode_messages(&mut buf).expect("decode");
    let SocketEvent::SetTargetList {
        connection_id,
        target_list,
    } = &decoded[0]
    else {
        panic!("expected SetTargetList");
    };
    let descriptors = descriptors_from_target_list(
        *connection_id,
        target_list,
        TransportOrigin::TcpInspectorServer {
            address: "127.0.0.1:2999".into(),
        },
    );
    assert_eq!(descriptors.len(), 3);
    assert_eq!(descriptors[0].key, TargetKey::from_ids(1, 2));
    assert_eq!(descriptors[1].key, TargetKey::from_ids(1, 3));
    assert_eq!(descriptors[2].key, TargetKey::from_ids(1, 4));
    assert!(descriptors.iter().all(|d| d.kind == TargetType::WebPage));
}

#[test]
fn empty_target_list_is_ok() {
    let event = SocketEvent::SetTargetList {
        connection_id: 1,
        target_list: vec![],
    };
    let mut buf = encode_message(&event).expect("encode");
    let decoded = decode_messages(&mut buf).expect("decode");
    let SocketEvent::SetTargetList {
        connection_id,
        target_list,
    } = &decoded[0]
    else {
        panic!("expected SetTargetList");
    };
    let descriptors = descriptors_from_target_list(
        *connection_id,
        target_list,
        TransportOrigin::TcpInspectorServer {
            address: "127.0.0.1:2999".into(),
        },
    );
    assert!(descriptors.is_empty());
}
