//! Precedence merge of JSON documents with provenance.
//!
//! Documents are merged from lowest to highest precedence. Objects merge key by
//! key, arrays are unioned (duplicates removed, every contributing source kept),
//! and scalars are replaced by the higher-precedence value, which records the
//! sources it shadowed.

use std::collections::BTreeMap;

use serde_json::Value;

/// Index into the list of loaded sources.
pub type SrcId = usize;

#[derive(Debug, Clone)]
pub enum Node {
    Object(BTreeMap<String, Node>),
    List(Vec<Item>),
    Scalar(Leaf),
}

/// One array element and every source that listed it.
#[derive(Debug, Clone)]
pub struct Item {
    pub value: Value,
    pub srcs: Vec<SrcId>,
}

#[derive(Debug, Clone)]
pub struct Leaf {
    pub value: Value,
    pub src: SrcId,
    pub shadowed: Vec<SrcId>,
}

impl Node {
    pub fn empty() -> Self {
        Node::Object(BTreeMap::new())
    }

    pub fn from_value(v: &Value, src: SrcId) -> Self {
        match v {
            Value::Object(m) => Node::Object(
                m.iter()
                    .map(|(k, v)| (k.clone(), Node::from_value(v, src)))
                    .collect(),
            ),
            Value::Array(a) => {
                let mut items: Vec<Item> = Vec::new();
                for x in a {
                    push_item(&mut items, x, src);
                }
                Node::List(items)
            }
            _ => Node::Scalar(Leaf {
                value: v.clone(),
                src,
                shadowed: Vec::new(),
            }),
        }
    }

    /// Merge `v` from `src` on top of this node (`src` has higher precedence).
    pub fn merge(&mut self, v: &Value, src: SrcId) {
        match (&mut *self, v) {
            (Node::Object(m), Value::Object(vm)) => {
                for (k, vv) in vm {
                    match m.get_mut(k) {
                        Some(n) => n.merge(vv, src),
                        None => {
                            m.insert(k.clone(), Node::from_value(vv, src));
                        }
                    }
                }
            }
            (Node::List(items), Value::Array(a)) => {
                for x in a {
                    push_item(items, x, src);
                }
            }
            (old, _) => {
                let shadowed = old.sources();
                let mut new = Node::from_value(v, src);
                if let Node::Scalar(leaf) = &mut new {
                    leaf.shadowed = shadowed;
                }
                *old = new;
            }
        }
    }

    /// Look up a dotted path such as `permissions.allow`.
    pub fn get(&self, path: &str) -> Option<&Node> {
        let mut node = self;
        for part in path.split('.') {
            match node {
                Node::Object(m) => node = m.get(part)?,
                _ => return None,
            }
        }
        Some(node)
    }

    pub fn items(&self, path: &str) -> &[Item] {
        match self.get(path) {
            Some(Node::List(items)) => items,
            _ => &[],
        }
    }

    pub fn leaf(&self, path: &str) -> Option<&Leaf> {
        match self.get(path) {
            Some(Node::Scalar(leaf)) => Some(leaf),
            _ => None,
        }
    }

    /// All sources that contributed anything to this node.
    fn sources(&self) -> Vec<SrcId> {
        let mut out = Vec::new();
        match self {
            Node::Object(m) => m.values().for_each(|n| out.extend(n.sources())),
            Node::List(items) => items.iter().for_each(|i| out.extend(&i.srcs)),
            Node::Scalar(leaf) => {
                out.push(leaf.src);
                out.extend(&leaf.shadowed);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

fn push_item(items: &mut Vec<Item>, v: &Value, src: SrcId) {
    match items.iter_mut().find(|i| i.value == *v) {
        Some(item) if !item.srcs.contains(&src) => item.srcs.push(src),
        Some(_) => {}
        None => items.push(Item {
            value: v.clone(),
            srcs: vec![src],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scalars_take_higher_precedence_and_record_shadowed() {
        let mut n = Node::from_value(&json!({"model": "a"}), 0);
        n.merge(&json!({"model": "b"}), 1);
        let leaf = n.leaf("model").unwrap();
        assert_eq!(leaf.value, json!("b"));
        assert_eq!(leaf.src, 1);
        assert_eq!(leaf.shadowed, vec![0]);
    }

    #[test]
    fn arrays_union_and_keep_all_sources() {
        let mut n = Node::from_value(&json!({"p": {"allow": ["Read", "Edit"]}}), 0);
        n.merge(&json!({"p": {"allow": ["Edit", "Bash(ls *)"]}}), 1);
        let items = n.items("p.allow");
        assert_eq!(items.len(), 3);
        assert_eq!(items[1].srcs, vec![0, 1]);
    }
}
