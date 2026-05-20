use std::io;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;

use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::{self, Instant};

use super::clock::Clock;
use super::{Event, EventMeta};

const DEFAULT_CHANNEL_CAPACITY: usize = 1_024;
const FLUSH_EVENT_COUNT: usize = 64;
const FLUSH_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone)]
pub struct EventEmitter {
    tx: mpsc::Sender<Event>,
    clock: Clock,
    next_seq: Arc<AtomicU64>,
    send_gate: Arc<Mutex<()>>,
}

#[derive(Debug)]
pub struct EventWriter {
    join: JoinHandle<io::Result<()>>,
}

impl EventEmitter {
    pub fn stdout() -> (Self, EventWriter) {
        Self::with_writer(tokio::io::stdout())
    }

    pub fn with_writer<W>(writer: W) -> (Self, EventWriter)
    where
        W: AsyncWrite + Send + Unpin + 'static,
    {
        Self::with_writer_and_capacity(writer, DEFAULT_CHANNEL_CAPACITY)
    }

    pub fn with_writer_and_capacity<W>(writer: W, capacity: usize) -> (Self, EventWriter)
    where
        W: AsyncWrite + Send + Unpin + 'static,
    {
        assert!(capacity > 0, "event channel capacity must be non-zero");

        let (tx, rx) = mpsc::channel(capacity);
        let join = tokio::spawn(write_events(rx, writer));
        let emitter = Self {
            tx,
            clock: Clock::new(),
            next_seq: Arc::new(AtomicU64::new(1)),
            send_gate: Arc::new(Mutex::new(())),
        };

        (emitter, EventWriter { join })
    }

    pub async fn emit(&self, mut event: Event) -> io::Result<()> {
        let _send_gate = self.send_gate.lock().await;
        let timestamp = self.clock.now();
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        event.set_meta(EventMeta {
            ts_ns: timestamp.ts_ns,
            ts_wall: timestamp.ts_wall,
            seq,
        });

        self.tx
            .send(event)
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "event writer task is closed"))
    }
}

impl EventWriter {
    pub async fn wait(self) -> io::Result<()> {
        self.join.await.map_err(|error| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("event writer task failed to join: {error}"),
            )
        })?
    }
}

async fn write_events<W>(mut rx: mpsc::Receiver<Event>, writer: W) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let mut writer = BufWriter::new(writer);
    let mut pending_events = 0usize;
    let mut flush_deadline: Option<Instant> = None;

    loop {
        if pending_events == 0 {
            match rx.recv().await {
                Some(event) => {
                    write_event(&mut writer, &event).await?;
                    pending_events = 1;
                    flush_deadline = Some(Instant::now() + FLUSH_INTERVAL);
                }
                None => {
                    writer.flush().await?;
                    return Ok(());
                }
            }
        } else {
            let deadline = flush_deadline.expect("pending events must have a flush deadline");

            tokio::select! {
                event = rx.recv() => {
                    match event {
                        Some(event) => {
                            write_event(&mut writer, &event).await?;
                            pending_events += 1;
                            if pending_events >= FLUSH_EVENT_COUNT {
                                writer.flush().await?;
                                pending_events = 0;
                                flush_deadline = None;
                            }
                        }
                        None => {
                            writer.flush().await?;
                            return Ok(());
                        }
                    }
                }
                _ = time::sleep_until(deadline) => {
                    writer.flush().await?;
                    pending_events = 0;
                    flush_deadline = None;
                }
            }
        }
    }
}

async fn write_event<W>(writer: &mut BufWriter<W>, event: &Event) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let json = serde_json::to_vec(event).map_err(io::Error::other)?;
    writer.write_all(&json).await?;
    writer.write_all(b"\n").await
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use std::sync::Mutex;
    use std::task::{Context, Poll};

    use serde_json::Value;

    use super::*;

    #[derive(Clone, Default)]
    struct SharedBuffer {
        bytes: Arc<Mutex<Vec<u8>>>,
    }

    impl SharedBuffer {
        fn contents(&self) -> Vec<u8> {
            self.bytes.lock().expect("buffer mutex poisoned").clone()
        }
    }

    impl AsyncWrite for SharedBuffer {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            self.bytes
                .lock()
                .expect("buffer mutex poisoned")
                .extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    fn placeholder_event() -> Event {
        Event::TextDelta {
            meta: EventMeta {
                ts_ns: 0,
                ts_wall: String::new(),
                seq: 0,
            },
            turn: 1,
            text: "chunk".to_string(),
        }
    }

    #[tokio::test]
    async fn writes_all_events_and_assigns_monotonic_sequence_numbers() {
        let buffer = SharedBuffer::default();
        let (emitter, writer) = EventEmitter::with_writer(buffer.clone());

        for _ in 0..1_000 {
            emitter.emit(placeholder_event()).await.unwrap();
        }
        drop(emitter);
        writer.wait().await.unwrap();

        let output = String::from_utf8(buffer.contents()).unwrap();
        let lines = output.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 1_000);

        for (index, line) in lines.into_iter().enumerate() {
            let value: Value = serde_json::from_str(line).unwrap();
            let seq = value["seq"].as_u64().expect("event seq must be a u64");
            let expected = index as u64 + 1;
            assert_eq!(seq, expected);
        }
    }
}
