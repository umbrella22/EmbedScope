//! Read-only presentation of application-supplied evidence on the log channel.
use escope_host::{LogRecord, UiNode};
use serde::Deserialize;

const TARGET: &str = "escope.ui.evidence.v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ResultKind {
    InputUnobserved,
    Rejected,
    Unchanged,
    PreviewUnconfirmed,
    Presented,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Evidence<'a> {
    pub control: &'a str,
    pub result: ResultKind,
    pub frame: Option<u64>,
}

impl<'a> Evidence<'a> {
    pub(super) fn from_log(log: &'a LogRecord, session: u32) -> Option<Self> {
        if log.session != session || log.target != TARGET || log.truncated {
            return None;
        }
        let evidence: Self = serde_json::from_str(&log.message).ok()?;
        match (evidence.result, evidence.frame) {
            (ResultKind::Presented, Some(frame)) if frame != 0 => Some(evidence),
            (ResultKind::Presented, _) | (_, Some(_)) => None,
            (_, None) => Some(evidence),
        }
    }
}

pub(super) fn control_key(node: &UiNode) -> Option<&str> {
    let key = node.name.strip_prefix("action:")?;
    (!key.is_empty()
        && key.len() <= 48
        && key.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_')
        }))
    .then_some(key)
}

pub(super) fn unique_control<'a>(nodes: &'a [UiNode], key: &str) -> Option<&'a UiNode> {
    let mut matches = nodes.iter().filter(|node| control_key(node) == Some(key));
    let node = matches.next()?;
    matches.next().is_none().then_some(node)
}

pub(super) fn selection_after_refresh(
    old: &[UiNode],
    new: &[UiNode],
    selected: Option<u32>,
) -> Option<u32> {
    let previous = old.iter().find(|node| Some(node.id) == selected)?;
    if new
        .iter()
        .any(|n| n.id == previous.id && n.revision == previous.revision)
    {
        return selected;
    }
    // A native id is revision scoped. Only a unique, explicitly tagged
    // application key can carry selection across a new tree revision.
    let key = control_key(previous)?;
    unique_control(old, key)?;
    let current = unique_control(new, key)?;
    (current.kind == previous.kind).then_some(current.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u32, revision: u32, name: &str) -> UiNode {
        UiNode {
            id,
            revision,
            name: name.into(),
            kind: "slider".into(),
            parent: 0,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            flags: 0,
        }
    }

    #[test]
    fn selection_never_follows_recycled_or_ambiguous_ids() {
        let name = "action:settings.brightness";
        let old = [node(8, 1, name)];
        let reused = node(8, 2, "another object");
        assert_eq!(
            selection_after_refresh(&old, std::slice::from_ref(&reused), Some(8)),
            None
        );
        let new = [reused, node(12, 2, name)];
        assert_eq!(selection_after_refresh(&old, &new, Some(8)), Some(12));
        let duplicates = [node(12, 2, name), node(13, 2, name)];
        assert_eq!(selection_after_refresh(&old, &duplicates, Some(8)), None);
    }

    #[test]
    fn evidence_requires_current_session_and_a_valid_result_frame_pair() {
        let mut log = LogRecord {
            session: 7,
            seq: 1,
            timestamp_ms: 10,
            level: 3,
            target: TARGET.into(),
            message: r#"{"control":"settings.brightness","result":"presented","frame":42}"#.into(),
            dropped: 0,
            truncated: false,
        };
        assert_eq!(Evidence::from_log(&log, 7).unwrap().frame, Some(42));
        assert!(Evidence::from_log(&log, 6).is_none());
        log.truncated = true;
        assert!(Evidence::from_log(&log, 7).is_none());
        log.truncated = false;
        log.message =
            r#"{"control":"settings.brightness","result":"presented","frame":null}"#.into();
        assert!(Evidence::from_log(&log, 7).is_none());
    }
}
