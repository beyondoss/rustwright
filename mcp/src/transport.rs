use std::sync::Arc;

use rmcp::{
    RoleServer,
    model::{
        ClientJsonRpcMessage, ClientRequest, ErrorCode, ErrorData, RequestId, ServerJsonRpcMessage,
    },
    transport::Transport,
};
use serde::Serialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, Stdin, Stdout},
    sync::Mutex,
};

const SERVER_NOT_INITIALIZED: i32 = -32002;
const MAX_STDIO_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Newline-delimited stdio with strict pre-initialize and parse-error responses.
///
/// The SDK still owns initialization and all post-initialize protocol routing. This
/// adapter only converts input that the stock transport drops or terminates on into
/// JSON-RPC errors before passing the next valid message to the SDK.
pub(crate) struct LifecycleStdio<R = Stdin, W = Stdout> {
    input: BufReader<R>,
    output: Arc<Mutex<W>>,
    line: Vec<u8>,
    initialize_seen: bool,
    max_frame_bytes: usize,
    discarding_oversized: bool,
}

impl LifecycleStdio<Stdin, Stdout> {
    pub(crate) fn new() -> Self {
        Self::from_io(tokio::io::stdin(), tokio::io::stdout())
    }
}

impl<R, W> LifecycleStdio<R, W>
where
    R: Send + AsyncRead + Unpin,
    W: Send + AsyncWrite + Unpin + 'static,
{
    fn from_io(input: R, output: W) -> Self {
        Self::from_io_with_limit(input, output, MAX_STDIO_FRAME_BYTES)
    }

    fn from_io_with_limit(input: R, output: W, max_frame_bytes: usize) -> Self {
        Self {
            input: BufReader::new(input),
            output: Arc::new(Mutex::new(output)),
            line: Vec::new(),
            initialize_seen: false,
            max_frame_bytes,
            discarding_oversized: false,
        }
    }

    fn send_error(
        &self,
        error: ErrorData,
        id: Option<RequestId>,
    ) -> impl Future<Output = std::io::Result<()>> + Send + 'static {
        send_frame(
            Arc::clone(&self.output),
            ServerJsonRpcMessage::error(error, id),
        )
    }

    fn reject_before_initialize(
        &self,
        id: RequestId,
    ) -> impl Future<Output = std::io::Result<()>> + Send + 'static {
        self.send_error(
            ErrorData::new(
                ErrorCode(SERVER_NOT_INITIALIZED),
                "Server not initialized",
                None,
            ),
            Some(id),
        )
    }

    async fn read_newline_frame(&mut self) -> Option<FrameRead> {
        loop {
            let buf = match self.input.fill_buf().await {
                Ok(buf) => buf,
                Err(error) => {
                    eprintln!("stdio read failed: {error}");
                    return None;
                }
            };
            if buf.is_empty() {
                return None;
            }

            if self.discarding_oversized {
                if let Some(newline) = buf.iter().position(|byte| *byte == b'\n') {
                    self.input.consume(newline + 1);
                    self.line.clear();
                    self.discarding_oversized = false;
                    return Some(FrameRead::Oversized);
                }
                let consumed = buf.len();
                self.input.consume(consumed);
                continue;
            }

            if let Some(newline) = buf.iter().position(|byte| *byte == b'\n') {
                let add = newline + 1;
                if self.line.len().saturating_add(add) > self.max_frame_bytes {
                    self.input.consume(add);
                    self.line.clear();
                    return Some(FrameRead::Oversized);
                }
                self.line.extend_from_slice(&buf[..add]);
                self.input.consume(add);
                return Some(FrameRead::Complete);
            }

            let add = buf.len();
            if self.line.len().saturating_add(add) > self.max_frame_bytes {
                self.input.consume(add);
                self.line.clear();
                self.discarding_oversized = true;
                continue;
            }
            self.line.extend_from_slice(buf);
            self.input.consume(add);
        }
    }
}

enum FrameRead {
    Complete,
    Oversized,
}

impl<R, W> Transport<RoleServer> for LifecycleStdio<R, W>
where
    R: Send + AsyncRead + Unpin,
    W: Send + AsyncWrite + Unpin + 'static,
{
    type Error = std::io::Error;

    fn send(
        &mut self,
        item: ServerJsonRpcMessage,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        send_frame(Arc::clone(&self.output), item)
    }

    async fn receive(&mut self) -> Option<ClientJsonRpcMessage> {
        loop {
            match self.read_newline_frame().await? {
                FrameRead::Oversized => {
                    if let Err(write_error) = self
                        .send_error(
                            ErrorData::invalid_request("frame too large", None),
                            None,
                        )
                        .await
                    {
                        eprintln!("stdio error response failed: {write_error}");
                        return None;
                    }
                    continue;
                }
                FrameRead::Complete => {}
            }

            let parsed = {
                let mut line = self.line.as_slice();
                while matches!(line.last(), Some(b'\n' | b'\r')) {
                    line = &line[..line.len() - 1];
                }
                if line.is_empty() {
                    self.line.clear();
                    continue;
                }
                serde_json::from_slice::<ClientJsonRpcMessage>(line)
            };
            // `read_newline_frame` may be cancelled after appending a partial frame.
            // Keep that buffer across calls, and clear it synchronously only once a
            // complete newline-delimited frame has been parsed.
            self.line.clear();

            let message = match parsed {
                Ok(message) => message,
                Err(error) => {
                    let error_data = match error.classify() {
                        serde_json::error::Category::Syntax | serde_json::error::Category::Eof => {
                            ErrorData::parse_error("Parse error", None)
                        }
                        serde_json::error::Category::Data | serde_json::error::Category::Io => {
                            ErrorData::invalid_request("Invalid request", None)
                        }
                    };
                    if let Err(write_error) = self.send_error(error_data, None).await {
                        eprintln!("stdio error response failed: {write_error}");
                        return None;
                    }
                    continue;
                }
            };

            if !self.initialize_seen {
                match &message {
                    ClientJsonRpcMessage::Request(request)
                        if matches!(request.request, ClientRequest::InitializeRequest(_)) =>
                    {
                        // The SDK sends InitializeResult before asking this transport for
                        // another message, so setting this here still gates the full handshake.
                        self.initialize_seen = true;
                    }
                    ClientJsonRpcMessage::Request(request)
                        if matches!(request.request, ClientRequest::PingRequest(_)) => {}
                    ClientJsonRpcMessage::Request(request) => {
                        if let Err(error) = self.reject_before_initialize(request.id.clone()).await
                        {
                            eprintln!("stdio lifecycle response failed: {error}");
                            return None;
                        }
                        continue;
                    }
                    _ => continue,
                }
            }

            return Some(message);
        }
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        self.output.lock().await.flush().await
    }
}

async fn send_frame<W, T>(output: Arc<Mutex<W>>, item: T) -> std::io::Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize + Send + 'static,
{
    let mut frame = serde_json::to_vec(&item).map_err(std::io::Error::other)?;
    frame.push(b'\n');
    let mut output = output.lock().await;
    output.write_all(&frame).await?;
    output.flush().await
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn cancelled_receive_resumes_partial_frame() {
        let (mut client, input) = tokio::io::duplex(256);
        let mut transport = LifecycleStdio::from_io(input, tokio::io::sink());
        let frame = br#"{"jsonrpc":"2.0","id":7,"method":"ping","params":{}}
"#;
        let split = 24;

        client
            .write_all(&frame[..split])
            .await
            .expect("write partial frame");
        assert!(
            tokio::time::timeout(Duration::from_millis(20), transport.receive())
                .await
                .is_err(),
            "partial frame must keep receive pending"
        );
        assert_eq!(transport.line, frame[..split]);

        client
            .write_all(&frame[split..])
            .await
            .expect("finish frame");
        let message = transport.receive().await.expect("receive resumed frame");
        let ClientJsonRpcMessage::Request(request) = message else {
            panic!("expected request");
        };
        assert_eq!(request.id, RequestId::Number(7));
        assert!(matches!(request.request, ClientRequest::PingRequest(_)));
        assert!(transport.line.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn oversized_frame_is_rejected_without_growing_the_buffer() {
        let (mut client, input) = tokio::io::duplex(1024);
        let (output, mut server_out) = tokio::io::duplex(8 * 1024);
        let mut transport = LifecycleStdio::from_io_with_limit(input, output, 32);
        let ping = br#"{"jsonrpc":"2.0","id":8,"method":"ping","params":{}}
"#;
        let mut inbound = Vec::from([b'x'; 40]);
        inbound.push(b'\n');
        inbound.extend_from_slice(ping);
        client.write_all(&inbound).await.expect("write frames");
        client.shutdown().await.expect("close stdin after frames");

        // Drain the error reply while receive() writes it so a full duplex
        // buffer cannot stall the single consumer loop.
        let drain = tokio::spawn(async move {
            let mut rejected = Vec::new();
            let mut buf = [0_u8; 512];
            loop {
                match server_out.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(read) => {
                        rejected.extend_from_slice(&buf[..read]);
                        if rejected.contains(&b'\n') {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            rejected
        });

        let message = tokio::time::timeout(Duration::from_secs(2), transport.receive())
            .await
            .expect("receive should not stall after an oversized frame")
            .expect("receive ping after cap");
        let ClientJsonRpcMessage::Request(request) = message else {
            panic!("expected ping request");
        };
        assert_eq!(request.id, RequestId::Number(8));
        assert!(transport.line.is_empty());
        assert!(!transport.discarding_oversized);

        let rejected = drain.await.expect("drain error reply");
        let text = String::from_utf8_lossy(&rejected);
        assert!(text.contains("frame too large"), "{text}");
    }
}
