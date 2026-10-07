//! Shared SSE stream parsing built on futures::stream::unfold.
//!
//! Robustness rules (tested in tests/adapters.rs):
//! - frames split across arbitrary network chunks reassemble correctly;
//! - multibyte UTF-8 split across chunks is decoded without corruption
//!   (only complete lines are decoded, and a newline cannot appear inside a
//!   multibyte sequence);
//! - provider error frames surface as ModelError::StreamError;
//! - premature EOF still yields a terminal Completed event carrying whatever
//!   usage was seen and ended_with_done=false so callers cannot mistake an
//!   unaccounted stream for a free successful one.
//!
//! The parser is written with the maintained futures unfold primitive: the
//! state machine is plain data moved through an async block, so there is no
//! manual Pin handling and no unsafe code.

use crate::types::{ModelError, StreamEvent, Usage};
use futures::stream::BoxStream;
use futures::StreamExt;
use serde_json::Value;
use std::collections::VecDeque;

struct State<S, F> {
    inner: S,
    buffer: Vec<u8>,
    pending: VecDeque<Result<StreamEvent, ModelError>>,
    usage: Option<Usage>,
    done: bool,
    saw_done_marker: bool,
    on_data: F,
}

fn process_line<S, F>(state: &mut State<S, F>, line_bytes: Vec<u8>)
where
    F: FnMut(&Value, &mut Option<Usage>, &mut Vec<StreamEvent>),
{
    let line = match String::from_utf8(line_bytes) {
        Ok(line) => line,
        Err(_) => return,
    };
    let Some(data) = line.strip_prefix("data:") else {
        return;
    };
    let data = data.trim_start();
    if data == "[DONE]" {
        state.saw_done_marker = true;
        state.done = true;
        let usage = state.usage;
        state.pending.push_back(Ok(StreamEvent::Completed {
            usage,
            ended_with_done: true,
        }));
        return;
    }
    match serde_json::from_str::<Value>(data) {
        Ok(value) => {
            if value.get("error").map(|e| !e.is_null()).unwrap_or(false) {
                state.done = true;
                state.pending.push_back(Err(ModelError::StreamError));
                return;
            }
            let mut out = Vec::new();
            (state.on_data)(&value, &mut state.usage, &mut out);
            state.pending.extend(out.into_iter().map(Ok));
        }
        Err(_) => {}
    }
}

pub fn parse<S, F>(source: S, on_data: F) -> BoxStream<'static, Result<StreamEvent, ModelError>>
where
    S: futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin + Send + 'static,
    F: FnMut(&Value, &mut Option<Usage>, &mut Vec<StreamEvent>) + Send + 'static,
{
    let state = State {
        inner: source,
        buffer: Vec::new(),
        pending: VecDeque::new(),
        usage: None,
        done: false,
        saw_done_marker: false,
        on_data,
    };
    futures::stream::unfold(state, |mut state| async move {
        loop {
            if let Some(event) = state.pending.pop_front() {
                return Some((event, state));
            }
            if state.done {
                return None;
            }
            // Complete lines only; partial multibyte sequences stay buffered.
            if let Some(newline) = state.buffer.iter().position(|b| *b == b'\n') {
                let mut line_bytes: Vec<u8> = state.buffer.drain(..newline + 1).collect();
                if line_bytes.last() == Some(&b'\n') {
                    line_bytes.pop();
                }
                if line_bytes.last() == Some(&b'\r') {
                    line_bytes.pop();
                }
                process_line(&mut state, line_bytes);
                continue;
            }
            match state.inner.next().await {
                Some(Ok(chunk)) => {
                    state.buffer.extend_from_slice(&chunk);
                }
                Some(Err(_err)) => {
                    state.done = true;
                    // Sanitized: transport details are never surfaced.
                    state.pending.push_back(Err(ModelError::Provider));
                }
                None => {
                    state.done = true;
                    let usage = state.usage;
                    state.pending.push_back(Ok(StreamEvent::Completed {
                        usage,
                        ended_with_done: state.saw_done_marker,
                    }));
                }
            }
        }
    })
    .boxed()
}
