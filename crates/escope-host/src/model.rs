use crate::{decode, encode, Error, Result, Session};
use escope_protocol::{self as p, capability, Operation, Status};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Serialize)]
pub struct Hello {
    pub session: u32,
    pub capabilities: u32,
    pub max_payload: u32,
    pub protocol: u16,
    pub name: String,
    pub build_id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviceInfo {
    pub name: String,
    pub model: String,
    pub firmware: String,
    pub build_id: String,
    pub boot_id: u32,
    pub uptime_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Metrics {
    pub uptime_ms: u64,
    pub heap_free: Option<u64>,
    pub heap_min_free: Option<u64>,
    pub heap_largest_free: Option<u64>,
    pub psram_free: Option<u64>,
    pub frame_ms: Option<u32>,
    pub queue_depth: Option<u32>,
    pub dropped_logs: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct UiNode {
    pub id: u32,
    pub parent: u32,
    pub kind: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub flags: u32,
    pub revision: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct LogRecord {
    pub session: u32,
    pub seq: u64,
    pub timestamp_ms: u64,
    pub level: u8,
    pub target: String,
    pub message: String,
    pub dropped: u64,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CrashInfo {
    pub present: bool,
    pub reason: String,
    pub build_id: String,
    pub message: String,
    pub boot_id: u32,
}

impl Session {
    pub fn supports(&self, capability: u32) -> bool {
        self.hello().capabilities & capability == capability
    }
    pub(crate) fn require(&self, capability: u32) -> Result<()> {
        if self.supports(capability) {
            Ok(())
        } else {
            Err(Error::Device(Status::Unsupported))
        }
    }

    pub async fn info(&self) -> Result<DeviceInfo> {
        let bytes = self.request_raw(Operation::Info.into(), &[]).await?;
        let d: p::DeviceInfo<'_> = decode(&bytes)?;
        Ok(DeviceInfo {
            name: d.name.into(),
            model: d.model.into(),
            firmware: d.firmware.into(),
            build_id: d.build_id.into(),
            boot_id: d.boot_id,
            uptime_ms: d.uptime_ms,
        })
    }

    pub async fn metrics(&self) -> Result<Metrics> {
        self.require(capability::METRICS)?;
        let bytes = self.request_raw(Operation::Metrics.into(), &[]).await?;
        let m: p::Metrics = decode(&bytes)?;
        Ok(Metrics {
            uptime_ms: m.uptime_ms,
            heap_free: m.heap_free,
            heap_min_free: m.heap_min_free,
            heap_largest_free: m.heap_largest_free,
            psram_free: m.psram_free,
            frame_ms: m.frame_ms,
            queue_depth: m.queue_depth,
            dropped_logs: m.dropped_logs,
        })
    }

    pub async fn start_logs(&self, max_level: u8) -> Result<()> {
        self.require(capability::LOGS)?;
        self.request_raw(
            Operation::LogsSubscribe.into(),
            &encode(&p::LogFilter { max_level })?,
        )
        .await?;
        Ok(())
    }

    pub async fn stop_logs(&self) -> Result<()> {
        self.request_raw(Operation::LogsUnsubscribe.into(), &[])
            .await?;
        Ok(())
    }

    pub async fn ui_tree(&self) -> Result<Vec<UiNode>> {
        self.require(capability::UI_TREE)?;
        for _ in 0..3 {
            match self.tree_once().await {
                Err(Error::Device(Status::Stale)) => continue,
                result => return result,
            }
        }
        Err(Error::Device(Status::Stale))
    }

    async fn tree_once(&self) -> Result<Vec<UiNode>> {
        let mut query = p::TreeQuery::default();
        let mut nodes = Vec::new();
        let mut cursors = HashSet::new();
        let mut ids = HashSet::new();
        loop {
            if !cursors.insert(query.cursor) || nodes.len() >= 4096 {
                return Err(Error::Protocol(
                    "tree exceeds traversal limit or repeats a cursor".into(),
                ));
            }
            let bytes = self
                .request_raw(Operation::UiTree.into(), &encode(&query)?)
                .await?;
            let node: p::UiNode<'_> = decode(&bytes)?;
            if node.revision == 0 || (query.revision != 0 && node.revision != query.revision) {
                return Err(Error::Device(Status::Stale));
            }
            if node.id == 0 || !ids.insert(node.id) {
                return Err(Error::Protocol("invalid or duplicate node identity".into()));
            }
            query.revision = node.revision;
            nodes.push(UiNode {
                id: node.id,
                parent: node.parent,
                kind: node.kind.into(),
                name: node.name.into(),
                x: node.x,
                y: node.y,
                width: node.width,
                height: node.height,
                flags: node.flags,
                revision: node.revision,
            });
            match node.next_cursor {
                Some(next) => query.cursor = next,
                None => break,
            }
        }
        tree_depths(&nodes)?;
        Ok(nodes)
    }

    pub async fn tap(&self, x: u32, y: u32, hold_ms: u32) -> Result<()> {
        self.require(capability::INPUT)?;
        self.request_raw(
            Operation::InputTap.into(),
            &encode(&p::InputTap { x, y, hold_ms })?,
        )
        .await?;
        Ok(())
    }

    pub async fn crash(&self) -> Result<CrashInfo> {
        self.require(capability::CRASH)?;
        let bytes = self.request_raw(Operation::Crash.into(), &[]).await?;
        let c: p::CrashInfo<'_> = decode(&bytes)?;
        Ok(CrashInfo {
            present: c.present,
            reason: c.reason.into(),
            build_id: c.build_id.into(),
            message: c.message.into(),
            boot_id: c.boot_id,
        })
    }
}

impl crate::Event {
    pub fn log(&self) -> Result<LogRecord> {
        if self.operation != Operation::Log as u16 {
            return Err(Error::Protocol("event is not a log record".into()));
        }
        let l: p::LogRecord<'_> = decode(&self.payload)?;
        Ok(LogRecord {
            session: self.session,
            seq: l.seq,
            timestamp_ms: l.timestamp_ms,
            level: l.level,
            target: l.target.into(),
            message: l.message.into(),
            dropped: l.dropped,
            truncated: l.truncated,
        })
    }
}

pub fn capability_names(mask: u32) -> Vec<&'static str> {
    [
        (capability::LOGS, "logs"),
        (capability::METRICS, "metrics"),
        (capability::UI_TREE, "ui.tree"),
        (capability::INPUT, "input.tap"),
        (capability::CAPTURE, "display.capture"),
        (capability::CRASH, "crash"),
    ]
    .into_iter()
    .filter_map(|(bit, name)| (mask & bit != 0).then_some(name))
    .collect()
}

/// Validate parent paths and cache depths in linear expected time. This also
/// accepts providers that enumerate children before their parents.
pub fn tree_depths(nodes: &[UiNode]) -> Result<HashMap<u32, usize>> {
    let mut parents = HashMap::with_capacity(nodes.len());
    for node in nodes {
        if node.id == 0 || parents.insert(node.id, node.parent).is_some() {
            return Err(Error::Protocol("invalid or duplicate tree identity".into()));
        }
    }
    let mut depths = HashMap::with_capacity(nodes.len());
    for node in nodes {
        if depths.contains_key(&node.id) {
            continue;
        }
        let mut cursor = node.id;
        let mut path = Vec::new();
        let mut visited = HashSet::new();
        let mut depth = loop {
            if let Some(depth) = depths.get(&cursor) {
                break depth + 1;
            }
            if !visited.insert(cursor) {
                return Err(Error::Protocol("cycle in tree parents".into()));
            }
            path.push(cursor);
            let parent = *parents
                .get(&cursor)
                .ok_or_else(|| Error::Protocol("tree references a missing parent".into()))?;
            if parent == 0 {
                break 0;
            }
            cursor = parent;
        };
        for id in path.into_iter().rev() {
            depths.insert(id, depth);
            depth += 1;
        }
    }
    Ok(depths)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(id: u32, parent: u32) -> UiNode {
        UiNode {
            id,
            parent,
            kind: String::new(),
            name: String::new(),
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            flags: 0,
            revision: 1,
        }
    }
    #[test]
    fn validates_parent_paths_and_handles_deep_reverse_order_tree() {
        let mut nodes: Vec<_> = (1..=4096).map(|id| node(id, id - 1)).collect();
        nodes.reverse();
        assert_eq!(tree_depths(&nodes).unwrap()[&4096], 4095);
        assert!(tree_depths(&[node(1, 2), node(2, 1)]).is_err());
        assert!(tree_depths(&[node(1, 2)]).is_err());
    }
}
