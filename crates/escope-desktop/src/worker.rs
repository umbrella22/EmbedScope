use escope_host::{Capture, DeviceInfo, Hello, LogRecord, Metrics, Session, UiNode};
use std::{sync::mpsc, time::Duration};
use tokio::sync::{broadcast, mpsc as async_mpsc};

pub enum Command {
    Connect(String, bool),
    Disconnect,
    Refresh,
    Tap(u32, u32),
    Auto(bool),
}
pub enum Update {
    Connected(DeviceInfo, Hello),
    Snapshot(Vec<UiNode>, Option<Metrics>, Option<Capture>),
    Log(LogRecord),
    LogGap(u64),
    Error(String),
    Disconnected,
    Busy(bool),
}

pub fn spawn() -> (async_mpsc::Sender<Command>, mpsc::Receiver<Update>) {
    let (commands, mut receiver) = async_mpsc::channel(16);
    let (updates, output) = mpsc::sync_channel(64);
    std::thread::Builder::new()
        .name("escope-connection".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("host runtime");
            runtime.block_on(async {
                while let Some(command) = receiver.recv().await {
                    if let Command::Connect(endpoint, dtr) = command {
                        let _ = updates.send(Update::Busy(true));
                        match Session::connect_with_options(
                            &endpoint,
                            escope_host::ConnectOptions {
                                dtr: dtr.then_some(true),
                                ..Default::default()
                            },
                        )
                        .await
                        {
                            Ok(session) => {
                                if let Err(error) =
                                    connected(&session, &mut receiver, &updates).await
                                {
                                    let _ = updates.send(Update::Error(error.to_string()));
                                }
                                session.close();
                                let _ = updates.send(Update::Disconnected);
                            }
                            Err(error) => {
                                let _ = updates.send(Update::Error(error.to_string()));
                            }
                        }
                        let _ = updates.send(Update::Busy(false));
                    }
                }
            });
        })
        .expect("connection worker");
    (commands, output)
}

async fn snapshot(
    session: &Session,
    updates: &mpsc::SyncSender<Update>,
) -> escope_host::Result<()> {
    use escope_host::protocol::capability;
    let tree = if session.supports(capability::UI_TREE) {
        session.ui_tree().await?
    } else {
        Vec::new()
    };
    let metrics = if session.supports(capability::METRICS) {
        Some(session.metrics().await?)
    } else {
        None
    };
    let capture = if session.supports(capability::CAPTURE) {
        Some(session.capture().await?)
    } else {
        None
    };
    let _ = updates.send(Update::Snapshot(tree, metrics, capture));
    Ok(())
}

async fn connected(
    session: &Session,
    commands: &mut async_mpsc::Receiver<Command>,
    updates: &mpsc::SyncSender<Update>,
) -> escope_host::Result<()> {
    let info = session.info().await?;
    let _ = updates.send(Update::Connected(info, session.hello().clone()));
    let mut events = session.subscribe();
    let mut logs = false;
    if session.supports(escope_host::protocol::capability::LOGS) {
        session.start_logs(5).await?;
        logs = true;
    }
    if let Err(error) = snapshot(session, updates).await {
        let _ = updates.send(Update::Error(error.to_string()));
    }
    let _ = updates.send(Update::Busy(false));
    let mut auto = false;
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            command = commands.recv() => {
                let _ = updates.send(Update::Busy(true));
                let result = match command {
                    Some(Command::Disconnect) | None => return Ok(()),
                    Some(Command::Refresh) => snapshot(session, updates).await,
                    Some(Command::Tap(x,y)) => {
                        match session.tap(x,y,60).await { Ok(()) => snapshot(session, updates).await, Err(e) => Err(e) }
                    }
                    Some(Command::Auto(value)) => { auto = value; Ok(()) }
                    Some(Command::Connect(_, _)) => Ok(()),
                };
                if let Err(error) = result { let _ = updates.send(Update::Error(error.to_string())); }
                let _ = updates.send(Update::Busy(false));
            }
            event = events.recv(), if logs => {
                match event {
                    Ok(event) => if let Ok(log) = event.log() { let _ = updates.send(Update::Log(log)); },
                    Err(broadcast::error::RecvError::Lagged(n)) => { let _ = updates.send(Update::LogGap(n)); }
                    Err(_) => logs = false,
                }
            }
            _ = ticker.tick() => {
                if session.is_closed() { return Err(escope_host::Error::Disconnected); }
                if auto {
                    let _ = updates.send(Update::Busy(true));
                    if let Err(error) = snapshot(session, updates).await { let _ = updates.send(Update::Error(error.to_string())); }
                    let _ = updates.send(Update::Busy(false));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn worker_snapshot_and_input_use_real_session() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("tcp://{}", listener.local_addr().unwrap());
        let (stop, rx) = tokio::sync::watch::channel(false);
        let task = tokio::spawn(escope_host::simulator::serve(listener, rx));
        let session = Session::connect(&endpoint, 115200, Duration::from_secs(2))
            .await
            .unwrap();
        let (tx, rx) = mpsc::sync_channel(8);
        snapshot(&session, &tx).await.unwrap();
        match rx.try_recv().unwrap() {
            Update::Snapshot(tree, _, Some(frame)) => {
                assert_eq!(tree.len(), 3);
                assert_eq!(frame.meta.width, 320);
            }
            _ => panic!("snapshot"),
        }
        session.tap(100, 140, 60).await.unwrap();
        snapshot(&session, &tx).await.unwrap();
        match rx.try_recv().unwrap() {
            Update::Snapshot(tree, _, _) => assert_eq!(tree[1].name, "Counter: 1"),
            _ => panic!("snapshot"),
        }
        session.close();
        let _ = stop.send(true);
        task.await.unwrap().unwrap();
    }
}
