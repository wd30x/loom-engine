//! Reading one `loom serve` reply line (`docs/architecture.md` section 13).

use serde_json::Value;

/// A well-formed reply to the request with the expected id.
#[derive(Debug, PartialEq)]
pub enum Reply<'a> {
    /// The raw text of `result`, exactly as `loom serve` wrote it.
    Result(&'a str),
    Error {
        code: i64,
        message: String,
    },
}

/// Checks that `line` is the reply to request `id` and returns its result
/// or error. The result is cut out of the line, not re-serialized, so every
/// number keeps its exact text: a success reply always has the shape
/// `{"jsonrpc":"2.0","id":<id>,"result":<result>}`. Err describes a line
/// that is not such a reply.
pub fn parse_reply(line: &str, id: u64) -> Result<Reply<'_>, String> {
    let value: Value =
        serde_json::from_str(line).map_err(|e| format!("reply is not JSON ({e}): {line}"))?;
    if value["jsonrpc"] != "2.0" {
        return Err(format!("reply is not JSON-RPC 2.0: {line}"));
    }
    if value["id"].as_u64() != Some(id) {
        return Err(format!("reply is not for request {id}: {line}"));
    }
    if let Some(error) = value.get("error") {
        let code = error["code"]
            .as_i64()
            .ok_or_else(|| format!("error reply without a numeric code: {line}"))?;
        let message = error["message"].as_str().unwrap_or_default().to_owned();
        return Ok(Reply::Error { code, message });
    }
    let prefix = format!(r#"{{"jsonrpc":"2.0","id":{id},"result":"#);
    line.strip_prefix(&prefix)
        .and_then(|rest| rest.strip_suffix('}'))
        .map(Reply::Result)
        .ok_or_else(|| format!("reply is neither a result nor an error: {line}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_text_is_cut_out_unchanged() {
        let line = r#"{"jsonrpc":"2.0","id":3,"result":{"tick":30,"sim_time":0.5}}"#;
        assert_eq!(
            parse_reply(line, 3),
            Ok(Reply::Result(r#"{"tick":30,"sim_time":0.5}"#))
        );
    }

    #[test]
    fn nested_objects_and_result_inside_strings_do_not_confuse_the_cut() {
        let result = r#"{"a":{"b":{}},"path":"x\"result\":{}.png","e":{}}"#;
        let line = format!(r#"{{"jsonrpc":"2.0","id":12,"result":{result}}}"#);
        assert_eq!(parse_reply(&line, 12), Ok(Reply::Result(result)));
        // Numbers keep their text: no round trip through f64.
        let line = r#"{"jsonrpc":"2.0","id":1,"result":{"pitch":-0.2617994}}"#;
        assert_eq!(
            parse_reply(line, 1),
            Ok(Reply::Result(r#"{"pitch":-0.2617994}"#))
        );
    }

    #[test]
    fn error_reply_is_an_error_not_a_result() {
        let line = r#"{"jsonrpc":"2.0","id":4,"error":{"code":-32602,"message":"ticks must be at least 1"}}"#;
        assert_eq!(
            parse_reply(line, 4),
            Ok(Reply::Error {
                code: -32602,
                message: "ticks must be at least 1".to_owned()
            })
        );
    }

    #[test]
    fn reply_to_another_request_is_rejected() {
        let line = r#"{"jsonrpc":"2.0","id":5,"result":{}}"#;
        assert!(parse_reply(line, 6).is_err());
        assert!(
            parse_reply(
                r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"x"}}"#,
                6
            )
            .is_err()
        );
    }

    #[test]
    fn lines_that_are_not_replies_are_rejected() {
        for line in [
            "",
            "not json",
            r#"{"status":"error","error":"bad --size"}"#,
            r#"{"jsonrpc":"2.0","id":1}"#,
            r#"{"id":1,"jsonrpc":"2.0","result":{}}"#,
        ] {
            assert!(parse_reply(line, 1).is_err(), "{line}");
        }
    }
}
