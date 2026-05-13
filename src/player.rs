use std::process::Stdio;
use std::sync::mpsc as std_mpsc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use serde_json::json;

pub enum PlayerCommand {
    Play(String, u8),
    TogglePause,
    SetVolume(u8),
    Quit,
}

#[derive(Clone, Debug)]
pub struct MetadataUpdate {
    pub title: Option<String>,
    pub time_pos: Option<f64>,
    pub duration: Option<f64>,
    pub paused: bool,
    pub error: Option<String>,
}

pub struct Player {
    command_tx: mpsc::Sender<PlayerCommand>,
    metadata_rx: std_mpsc::Receiver<MetadataUpdate>,
}

impl Player {
    pub fn new() -> Self {
        let (command_tx, mut rx) = mpsc::channel::<PlayerCommand>(32);
        let (metadata_tx, metadata_rx) = std_mpsc::sync_channel::<MetadataUpdate>(2);

        let ipc_path = format!("/tmp/lofigirl_ipc_{}.sock", std::process::id());

        tokio::spawn(async move {
            let mut child: Option<Child> = None;
            let ipc = ipc_path.clone();
            // Token to cancel the previous metadata polling task
            let mut poll_cancel_tx: Option<tokio::sync::watch::Sender<bool>> = None;

            while let Some(cmd) = rx.recv().await {
                match cmd {
                    PlayerCommand::Play(url, vol) => {
                        // Cancel previous metadata polling task
                        if let Some(tx) = poll_cancel_tx.take() {
                            let _ = tx.send(true);
                        }

                        if child.is_some() {
                            send_ipc_command(&ipc, &serde_json::json!({ "command": ["quit"] })).await;
                        }

                        if let Some(mut c) = child.take() {
                            let _ = tokio::time::timeout(std::time::Duration::from_millis(500), c.wait()).await;
                            let _ = c.kill().await;
                            let _ = c.wait().await;
                        }

                        let _ = std::fs::remove_file(&ipc);

                        let log_file = match std::fs::File::create(std::env::temp_dir().join("mpv.log")) {
                            Ok(f) => f,
                            Err(_) => continue,
                        };

                        match Command::new("mpv")
                            .arg("--no-video")
                            // Live streams have no audio-only format; fall back to lowest
                            // quality muxed stream (144p ~269kbps) to minimize bandwidth/RAM
                            .arg("--ytdl-format=bestaudio/worst")
                            .arg("--demuxer-max-bytes=50MiB")
                            .arg("--demuxer-max-back-bytes=10MiB")
                            .arg(format!("--volume={}", vol))
                            .arg(format!("--input-ipc-server={}", ipc))
                            .arg("--quiet")
                            .arg(&url)
                            .stdin(Stdio::null())
                            .stdout(log_file.try_clone().unwrap())
                            .stderr(log_file)
                            .spawn()
                        {
                            Ok(c) => {
                                child = Some(c);

                                // Spawn cancellable metadata polling task
                                let poll_ipc = ipc.clone();
                                let meta_tx = metadata_tx.clone();
                                let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
                                poll_cancel_tx = Some(cancel_tx);

                                tokio::spawn(async move {
                                    // Wait for mpv to start and create the IPC socket
                                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                                    poll_metadata(poll_ipc, meta_tx, cancel_rx).await;
                                });
                            }
                            Err(e) => {
                                child = None;
                                let _ = metadata_tx.try_send(MetadataUpdate {
                                    title: None,
                                    time_pos: None,
                                    duration: None,
                                    paused: false,
                                    error: Some(format!("Failed to start mpv: {}", e)),
                                });
                            }
                        }
                    }
                    PlayerCommand::TogglePause => {
                        send_ipc_command(&ipc, &json!({ "command": ["cycle", "pause"] })).await;
                    }
                    PlayerCommand::SetVolume(vol) => {
                        send_ipc_command(&ipc, &json!({ "command": ["set_property", "volume", vol] })).await;
                    }
                    PlayerCommand::Quit => {
                        // Cancel polling task
                        if let Some(tx) = poll_cancel_tx.take() {
                            let _ = tx.send(true);
                        }
                        send_ipc_command(&ipc, &json!({ "command": ["quit"] })).await;
                        if let Some(mut c) = child.take() {
                            let _ = c.kill().await;
                            let _ = c.wait().await;
                        }
                        let _ = std::fs::remove_file(&ipc);
                        break;
                    }
                }
            }
        });

        Self { command_tx, metadata_rx }
    }

    pub fn play(&self, url: &str, vol: u8) {
        let _ = self.command_tx.try_send(PlayerCommand::Play(url.to_string(), vol));
    }

    pub fn toggle_pause(&self) {
        let _ = self.command_tx.try_send(PlayerCommand::TogglePause);
    }

    pub fn set_volume(&self, vol: u8) {
        let _ = self.command_tx.try_send(PlayerCommand::SetVolume(vol));
    }

    /// Drains the channel and returns only the most recent metadata update,
    /// discarding stale intermediate values.
    pub fn try_recv_metadata(&self) -> Option<MetadataUpdate> {
        let mut latest = None;
        while let Ok(meta) = self.metadata_rx.try_recv() {
            latest = Some(meta);
        }
        latest
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.command_tx.try_send(PlayerCommand::Quit);
    }
}

/// Send a single fire-and-forget IPC command (for volume, pause, quit).
/// Opens a short-lived connection — fine for infrequent user actions.
async fn send_ipc_command(ipc_path: &str, cmd: &serde_json::Value) {
    if let Ok(stream) = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        UnixStream::connect(ipc_path),
    )
    .await
        && let Ok(mut stream) = stream {
            let mut msg = cmd.to_string();
            msg.push('\n');
            let _ = stream.write_all(msg.as_bytes()).await;
            // Read the response to avoid broken pipe on mpv side
            let mut buf = vec![0u8; 512];
            let _ = tokio::time::timeout(
                std::time::Duration::from_millis(200),
                stream.read(&mut buf),
            ).await;
        }
}

/// Query a single property over an existing persistent connection.
/// Returns None if the query fails.
async fn query_property(
    writer: &mut tokio::io::WriteHalf<UnixStream>,
    reader: &mut BufReader<tokio::io::ReadHalf<UnixStream>>,
    property: &str,
    buf: &mut String,
) -> Option<serde_json::Value> {
    let cmd = json!({"command": ["get_property", property]});
    let mut msg = cmd.to_string();
    msg.push('\n');

    if writer.write_all(msg.as_bytes()).await.is_err() {
        return None;
    }

    // Read lines until we get a response with "data" (skip async events)
    for _ in 0..5 {
        buf.clear();
        match tokio::time::timeout(
            std::time::Duration::from_millis(300),
            reader.read_line(buf),
        ).await {
            Ok(Ok(0)) => return None, // EOF
            Ok(Ok(_)) => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(buf.trim()) {
                    // mpv sends async events too; skip them, we want {"data":..., "error":"success"}
                    if json.get("error").and_then(|e| e.as_str()) == Some("success") {
                        return Some(json);
                    }
                    if json.get("data").is_some() {
                        return Some(json);
                    }
                    // It's an event, keep reading
                    continue;
                }
            }
            _ => return None, // timeout or error
        }
    }
    None
}

/// Metadata polling loop using a PERSISTENT IPC connection.
/// Reconnects if the connection drops, but keeps it alive between polls.
async fn poll_metadata(
    ipc_path: String,
    tx: std_mpsc::SyncSender<MetadataUpdate>,
    mut cancel_rx: tokio::sync::watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut persistent_conn: Option<(
        tokio::io::WriteHalf<UnixStream>,
        BufReader<tokio::io::ReadHalf<UnixStream>>,
    )> = None;

    let mut buf = String::with_capacity(512);

    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = cancel_rx.changed() => {
                return; // Cancelled
            }
        }

        // Establish or reuse persistent connection
        let (writer, reader) = if let Some(ref mut conn) = persistent_conn {
            (&mut conn.0, &mut conn.1)
        } else {
            // Try to connect
            let stream = match tokio::time::timeout(
                std::time::Duration::from_millis(500),
                UnixStream::connect(&ipc_path),
            ).await {
                Ok(Ok(s)) => s,
                _ => continue, // retry next tick
            };

            let (read, write) = tokio::io::split(stream);
            persistent_conn = Some((write, BufReader::new(read)));
            let conn = persistent_conn.as_mut().unwrap();
            (&mut conn.0, &mut conn.1)
        };

        // Query all properties over the persistent connection
        let title = query_property(writer, reader, "media-title", &mut buf).await
            .and_then(|j| j["data"].as_str().map(|s| s.to_string()));

        let paused = query_property(writer, reader, "pause", &mut buf).await
            .and_then(|j| j["data"].as_bool())
            .unwrap_or(false);

        let time_pos = query_property(writer, reader, "time-pos", &mut buf).await
            .and_then(|j| j["data"].as_f64());

        let duration = query_property(writer, reader, "duration", &mut buf).await
            .and_then(|j| j["data"].as_f64());

        // If all queries returned None, connection is broken — drop and reconnect
        if title.is_none() && time_pos.is_none() && duration.is_none() {
            persistent_conn = None;
            continue;
        }

        let _ = tx.try_send(MetadataUpdate { title, time_pos, duration, paused, error: None });
    }
}