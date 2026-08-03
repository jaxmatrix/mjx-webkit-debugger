//! The identity dialect: WebKit's Remote Inspector protocol.
//!
//! "Identity" understates it slightly. The vocabulary passes through unchanged,
//! but **target multiplexing still has to be applied and undone**, and that is
//! not a no-op. A multi-process page routes frames through
//! `Target.sendMessageToTarget` / `Target.dispatchMessageFromTarget`, which
//! carry the real frame as a **JSON string inside a JSON object**.
//!
//! Getting this wrong is the failure mode described in `docs/PROTOCOL-NOTES.md`:
//! everything works against a simple page and every domain breaks the first
//! time a real multi-process site is opened.

use mjx_wk_protocol::{Domain, Frame};
use serde_json::{Value, json};

use crate::{Dialect, DialectError, DialectKind, NormalizedFrame, Support, TargetId};

/// WebKit's Remote Inspector protocol, spoken verbatim.
#[derive(Debug, Clone, Copy, Default)]
pub struct WebKitDialect;

/// The member that carries a frame *to* a sub-target.
const SEND_TO_TARGET: &str = "sendMessageToTarget";
/// The member that carries a frame *from* a sub-target.
const DISPATCH_FROM_TARGET: &str = "dispatchMessageFromTarget";

impl Dialect for WebKitDialect {
    fn kind(&self) -> DialectKind {
        DialectKind::WebKitRwi
    }

    fn encode(&self, frame: Frame, target: Option<&TargetId>) -> Result<Frame, DialectError> {
        let Some(target) = target else {
            return Ok(frame);
        };

        // Only requests are routed. Nothing else originates here.
        let Frame::Request { id, .. } = &frame else {
            return Ok(frame);
        };
        let outer_id = *id;

        let message = frame.to_json()?;
        Ok(Frame::Request {
            id: outer_id,
            method: format!("{}.{SEND_TO_TARGET}", Domain::Target.as_str()),
            params: json!({ "targetId": target.0, "message": message }),
        })
    }

    fn decode(&self, frame: Frame) -> Result<NormalizedFrame, DialectError> {
        let is_dispatch = frame
            .split_method()
            .is_some_and(|(d, m)| d == Domain::Target.as_str() && m == DISPATCH_FROM_TARGET);

        if !is_dispatch {
            return Ok(NormalizedFrame {
                frame,
                target: None,
            });
        }

        let Frame::Event { params, .. } = &frame else {
            return Err(DialectError::Envelope(
                "Target.dispatchMessageFromTarget arrived as something other than an event".into(),
            ));
        };

        let target = params
            .get("targetId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                DialectError::Envelope("dispatchMessageFromTarget has no targetId".into())
            })?
            .to_owned();

        // The inner frame is a *string*, not a nested object. Parsing it is the
        // whole job of this branch.
        let message = params
            .get("message")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                DialectError::Envelope("dispatchMessageFromTarget has no message".into())
            })?;

        let inner = Frame::from_json(message).map_err(|e| {
            DialectError::Envelope(format!("inner message is not a valid frame: {e}"))
        })?;

        Ok(NormalizedFrame {
            frame: inner,
            target: Some(TargetId(target)),
        })
    }

    fn supports(&self, _domain: Domain, _member: &str) -> Support {
        // This *is* the vocabulary, so anything expressible is expressible.
        //
        // Whether a given debuggee has the member is a different question with
        // a different owner: the session tracks what the debuggee announced.
        Support::Native
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mjx_wk_protocol::RequestId;

    #[test]
    fn an_unrouted_frame_passes_through_untouched() {
        let frame = Frame::from_json(r#"{"id":1,"method":"Debugger.enable","params":{}}"#).unwrap();
        let encoded = WebKitDialect.encode(frame.clone(), None).unwrap();
        assert_eq!(encoded, frame);
    }

    #[test]
    fn a_routed_request_is_wrapped_with_the_frame_as_a_json_string() {
        let frame = Frame::from_json(r#"{"id":7,"method":"Debugger.resume","params":{}}"#).unwrap();
        let encoded = WebKitDialect
            .encode(frame, Some(&TargetId("page-2".into())))
            .unwrap();

        let Frame::Request { id, method, params } = encoded else {
            panic!("expected a request");
        };
        assert_eq!(id, RequestId(7));
        assert_eq!(method, "Target.sendMessageToTarget");
        assert_eq!(params["targetId"], "page-2");

        // The inner frame must be a string, not a nested object — this is the
        // detail that breaks multi-process debugging when it is got wrong.
        let inner = params["message"]
            .as_str()
            .expect("message must be a string");
        assert_eq!(
            Frame::from_json(inner).unwrap().method(),
            Some("Debugger.resume")
        );
    }

    #[test]
    fn a_dispatched_frame_is_unwrapped_and_attributed_to_its_target() {
        let outer = format!(
            r#"{{"method":"Target.dispatchMessageFromTarget","params":{{"targetId":"w1","message":{}}}}}"#,
            serde_json::to_string(
                r#"{"method":"Debugger.paused","params":{"reason":"Breakpoint"}}"#
            )
            .unwrap()
        );
        let decoded = WebKitDialect
            .decode(Frame::from_json(&outer).unwrap())
            .unwrap();

        assert_eq!(decoded.target, Some(TargetId("w1".into())));
        assert_eq!(decoded.frame.method(), Some("Debugger.paused"));
    }

    #[test]
    fn a_flat_frame_decodes_with_no_target() {
        // Single-process pages never wrap anything, and that path must stay
        // free of envelope handling.
        let decoded = WebKitDialect
            .decode(Frame::from_json(r#"{"method":"Debugger.resumed","params":{}}"#).unwrap())
            .unwrap();
        assert!(decoded.target.is_none());
        assert_eq!(decoded.frame.method(), Some("Debugger.resumed"));
    }

    #[test]
    fn a_malformed_envelope_is_an_error_not_a_silent_drop() {
        for bad in [
            r#"{"method":"Target.dispatchMessageFromTarget","params":{"message":"{}"}}"#,
            r#"{"method":"Target.dispatchMessageFromTarget","params":{"targetId":"w1"}}"#,
            r#"{"method":"Target.dispatchMessageFromTarget","params":{"targetId":"w1","message":"not json"}}"#,
        ] {
            let result = WebKitDialect.decode(Frame::from_json(bad).unwrap());
            assert!(matches!(result, Err(DialectError::Envelope(_))), "{bad}");
        }
    }

    #[test]
    fn wrapping_then_unwrapping_returns_the_original_frame() {
        let original =
            Frame::from_json(r#"{"id":3,"method":"Runtime.evaluate","params":{"expression":"1"}}"#)
                .unwrap();
        let target = TargetId("t".into());

        let Frame::Request { params, .. } = WebKitDialect
            .encode(original.clone(), Some(&target))
            .unwrap()
        else {
            panic!("expected a request");
        };

        // Re-shape the outbound wrapper as the inbound one the debuggee would
        // send back, and check the round trip.
        let echoed = Frame::Event {
            method: "Target.dispatchMessageFromTarget".into(),
            params: json!({
                "targetId": target.0,
                "message": params["message"].as_str().unwrap(),
            }),
        };
        let decoded = WebKitDialect.decode(echoed).unwrap();
        assert_eq!(decoded.frame, original);
        assert_eq!(decoded.target, Some(target));
    }
}
