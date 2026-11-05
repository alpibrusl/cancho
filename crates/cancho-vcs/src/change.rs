//! A change: the commit of `docs/vcs-cli.md` section 3. A snapshot (`tree`),
//! its `parents`, who and when, and the part git has no field for, the
//! `intent`. Stored as canonical JSON (struct field order is the order here,
//! optional fields are omitted when empty so adding one later does not
//! change an old change's id).

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{Refusal, Result};
use crate::fsx::{hex, unhex};
use crate::object::{Kind, Store, is_id};

/// Bytes that are text when they can be, so the common case reads as text
/// in the store and a non-UTF-8 commit message survives a round trip.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Bytes(pub Vec<u8>);

impl Serialize for Bytes {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        match std::str::from_utf8(&self.0) {
            Ok(text) => s.serialize_str(text),
            Err(_) => {
                use serde::ser::SerializeMap;
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("hex", &hex(&self.0))?;
                m.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Bytes, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Text(String),
            Hex { hex: String },
        }
        match Repr::deserialize(d)? {
            Repr::Text(t) => Ok(Bytes(t.into_bytes())),
            Repr::Hex { hex } => unhex(&hex)
                .map(Bytes)
                .ok_or_else(|| serde::de::Error::custom("bad hex in a byte field")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intent {
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<String>,
}

/// What a change did to one declaration of a `.cho` file, derived at record time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclChange {
    pub path: String,
    pub name: String,
    /// `added`, `removed`, `signature-changed` or `body-changed`.
    pub kind: String,
}

/// An anchored edit operation (`docs/vcs-cli.md` section 3): applies only if
/// `old` occurs exactly once in the file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    pub path: String,
    pub old: String,
    pub new: String,
}

/// What a git commit has that a change must reproduce to get the same git
/// id: the header lines after `parent` (author, committer, encoding,
/// signatures, in their order) and the message, byte for byte.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCommit {
    pub headers: Bytes,
    pub message: Bytes,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub tree: String,
    pub parents: Vec<String>,
    pub author: String,
    pub time: i64,
    pub tz: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<Intent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decls: Vec<DeclChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<Edit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attestations: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<GitCommit>,
}

impl Change {
    pub fn encode(&self) -> Vec<u8> {
        // Plain data with string keys: serialisation cannot fail.
        serde_json::to_vec(self).unwrap_or_default()
    }

    pub fn save(&self, store: &Store) -> Result<String> {
        store.put(Kind::Change, &self.encode())
    }

    pub fn load(store: &Store, id: &str) -> Result<Change> {
        let bytes = store.get(Kind::Change, id)?;
        let c: Change = serde_json::from_slice(&bytes)
            .map_err(|e| Refusal::new("object-corrupt", format!("change {id}: {e}")))?;
        if !is_id(&c.tree) || c.parents.iter().any(|p| !is_id(p)) || c.parents.len() > 2 {
            return Err(Refusal::new("object-corrupt", format!("change {id}: bad tree or parents")));
        }
        Ok(c)
    }

    /// The first line of the intent prompt, for one-line listings.
    pub fn subject(&self) -> String {
        self.intent
            .as_ref()
            .map(|i| i.prompt.lines().next().unwrap_or("").to_owned())
            .unwrap_or_default()
    }
}
