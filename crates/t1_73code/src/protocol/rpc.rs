use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum FromClientMessage {
    Request {
        id: serde_json::Value,
        tag: String,
        payload: serde_json::Value,
        #[serde(default)]
        headers: Vec<(String, String)>,
        #[serde(default)]
        discard: Option<bool>,
    },
    Interrupt {
        #[serde(rename = "requestId")]
        request_id: serde_json::Value,
    },
    Ack {
        #[serde(rename = "requestId")]
        request_id: serde_json::Value,
    },
    Ping,
    Eof,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum FromServerMessage {
    Pong,
    Chunk {
        #[serde(rename = "requestId")]
        request_id: serde_json::Value,
        values: Vec<serde_json::Value>,
    },
    Exit {
        #[serde(rename = "requestId")]
        request_id: serde_json::Value,
        exit: RpcExit,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum RpcExit {
    Success {
        value: serde_json::Value,
    },
    Failure {
        cause: Vec<RpcCause>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum RpcCause {
    Fail {
        error: serde_json::Value,
    },
    Die {
        defect: serde_json::Value,
    },
    Interrupt {
        #[serde(rename = "fiberId")]
        fiber_id: Option<u64>,
    },
}

impl FromServerMessage {
    pub fn success(request_id: serde_json::Value, value: serde_json::Value) -> Self {
        Self::Exit {
            request_id,
            exit: RpcExit::Success { value },
        }
    }

    pub fn failure(request_id: serde_json::Value, error: serde_json::Value) -> Self {
        Self::Exit {
            request_id,
            exit: RpcExit::Failure {
                cause: vec![RpcCause::Fail { error }],
            },
        }
    }

    pub fn chunk(request_id: serde_json::Value, values: Vec<serde_json::Value>) -> Self {
        Self::Chunk {
            request_id,
            values,
        }
    }
}
