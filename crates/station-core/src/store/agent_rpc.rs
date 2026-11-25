use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Value,
}

pub fn encode_rpc_request(id: u64, method: &str, params: Value) -> String {
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    format!("{req}\n")
}

pub fn encode_rpc_notification(method: &str, params: Option<Value>) -> String {
    let note = match params {
        Some(p) => json!({ "jsonrpc": "2.0", "method": method, "params": p }),
        None => json!({ "jsonrpc": "2.0", "method": method }),
    };
    format!("{note}\n")
}

pub fn parse_rpc_line(line: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    if v.is_object() { Some(v) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_parses_jsonrpc_lines() {
        let s = encode_rpc_request(1, "initialize", json!({ "clientInfo": { "name": "agents-station" } }));
        assert!(s.ends_with('\n'));
        let parsed = parse_rpc_line(&s).unwrap();
        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["method"], "initialize");
    }
}
