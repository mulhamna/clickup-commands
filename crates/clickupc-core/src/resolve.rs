//! Names → ids for lists, spaces, folders, statuses, members, fields and
//! task types, with a disk cache (TTL 1h) and refresh-on-miss. Ambiguous
//! names are an error listing the qualified paths, never a silent pick.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    client::{convert_field_value, relation_value, ClickUpClient, FieldValue},
    config::ClickUpConfig,
    error::{ClickUpError, Result},
    model::{CustomField, List, User},
    task_ref::TaskRef,
};

pub const CACHE_TTL_SECS: i64 = 3600;

/// Up to 3 candidates with Jaro-Winkler similarity ≥ 0.8 (case-insensitive).
pub fn suggest(input: &str, candidates: &[String]) -> Vec<String> {
    let needle = input.to_lowercase();
    let mut scored: Vec<(f64, &String)> = candidates
        .iter()
        .map(|c| (strsim::jaro_winkler(&needle, &c.to_lowercase()), c))
        .filter(|(s, _)| *s >= 0.8)
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<String> = Vec::new();
    for (_, c) in scored {
        if !out.contains(c) {
            out.push(c.clone());
        }
        if out.len() == 3 {
            break;
        }
    }
    out
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HList {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HFolder {
    pub id: String,
    pub name: String,
    pub lists: Vec<HList>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HSpace {
    pub id: String,
    pub name: String,
    pub folders: Vec<HFolder>,
    /// Folderless lists.
    pub lists: Vec<HList>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Hierarchy {
    pub spaces: Vec<HSpace>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ListRef {
    pub id: String,
    pub name: String,
    pub space: Option<String>,
    pub folder: Option<String>,
}

impl ListRef {
    /// `Space/Folder/List` or `Space/List`.
    pub fn path(&self) -> String {
        [
            self.space.as_deref(),
            self.folder.as_deref(),
            Some(&self.name),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("/")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpaceRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FolderRef {
    pub id: String,
    pub name: String,
    pub space: String,
}

impl Hierarchy {
    pub fn lists(&self) -> Vec<ListRef> {
        let mut out = Vec::new();
        for s in &self.spaces {
            for f in &s.folders {
                for l in &f.lists {
                    out.push(ListRef {
                        id: l.id.clone(),
                        name: l.name.clone(),
                        space: Some(s.name.clone()),
                        folder: Some(f.name.clone()),
                    });
                }
            }
            for l in &s.lists {
                out.push(ListRef {
                    id: l.id.clone(),
                    name: l.name.clone(),
                    space: Some(s.name.clone()),
                    folder: None,
                });
            }
        }
        out
    }

    pub fn folders(&self) -> Vec<FolderRef> {
        self.spaces
            .iter()
            .flat_map(|s| {
                s.folders.iter().map(|f| FolderRef {
                    id: f.id.clone(),
                    name: f.name.clone(),
                    space: s.name.clone(),
                })
            })
            .collect()
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

#[derive(Serialize, Deserialize)]
struct Cached<T> {
    fetched_at: i64,
    data: T,
}

/// JSON files under `{cache_dir}/{workspace_id}/`.
#[derive(Debug, Clone)]
pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub fn new(cache_dir: PathBuf, workspace_id: &str) -> Self {
        Self {
            dir: cache_dir.join(workspace_id),
        }
    }

    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }

    fn load<T: DeserializeOwned>(&self, name: &str) -> Option<T> {
        let text = std::fs::read_to_string(self.dir.join(name)).ok()?;
        let c: Cached<T> = serde_json::from_str(&text).ok()?;
        (now_secs() - c.fetched_at < CACHE_TTL_SECS).then_some(c.data)
    }

    fn save<T: Serialize>(&self, name: &str, data: &T) {
        let path = self.dir.join(name);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let payload = Cached {
            fetched_at: now_secs(),
            data,
        };
        if let Ok(text) = serde_json::to_string(&payload) {
            // ponytail: cache write failures are non-fatal; next call refetches.
            let _ = std::fs::write(path, text);
        }
    }

    pub fn clear(&self) -> Result<()> {
        if self.dir.exists() {
            std::fs::remove_dir_all(&self.dir)?;
        }
        Ok(())
    }
}

pub struct Resolver<'a> {
    client: &'a ClickUpClient,
    cache: Cache,
    aliases: BTreeMap<String, String>,
    refresh: bool,
    hierarchy: Mutex<Option<(Hierarchy, bool)>>,
}

fn not_found(kind: &'static str, input: &str, names: &[String]) -> ClickUpError {
    ClickUpError::resolve(kind, input, suggest(input, names))
}

/// Pick the single item matching `pred`, or an ambiguity error.
fn unique<T: Clone>(
    kind: &'static str,
    input: &str,
    items: &[T],
    label: impl Fn(&T) -> String,
    pred: impl Fn(&T) -> bool,
) -> Result<Option<T>> {
    let hits: Vec<&T> = items.iter().filter(|i| pred(i)).collect();
    match hits.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some((*one).clone())),
        many => Err(ClickUpError::Resolve {
            kind,
            input: format!("{input}' is ambiguous; matches: '{}", {
                many.iter()
                    .map(|i| label(i))
                    .collect::<Vec<_>>()
                    .join("', '")
            }),
            suggestions: vec![],
        }),
    }
}

impl<'a> Resolver<'a> {
    pub fn new(client: &'a ClickUpClient, cfg: &ClickUpConfig) -> Result<Self> {
        Ok(Self {
            client,
            cache: Cache::new(cfg.cache_dir.clone(), client.workspace_id()?),
            aliases: cfg.aliases.clone(),
            refresh: false,
            hierarchy: Mutex::new(None),
        })
    }

    /// Bypass the disk cache (`--refresh`).
    pub fn with_refresh(mut self, refresh: bool) -> Self {
        self.refresh = refresh;
        self
    }

    pub fn client(&self) -> &ClickUpClient {
        self.client
    }

    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    /// Space → Folder → List tree. Cost: 1 + 2 × spaces requests.
    pub async fn hierarchy(&self, refresh: bool) -> Result<Hierarchy> {
        Ok(self.hierarchy_with_origin(refresh).await?.0)
    }

    /// Returns the tree and whether it came from the network just now.
    async fn hierarchy_with_origin(&self, refresh: bool) -> Result<(Hierarchy, bool)> {
        let refresh = refresh || self.refresh;
        if !refresh {
            if let Some(h) = self.hierarchy.lock().expect("hierarchy lock").clone() {
                return Ok(h);
            }
            if let Some(h) = self.cache.load::<Hierarchy>("hierarchy.json") {
                *self.hierarchy.lock().expect("hierarchy lock") = Some((h.clone(), false));
                return Ok((h, false));
            }
        }
        let mut tree = Hierarchy::default();
        for space in self.client.spaces(false).await? {
            let folders = self.client.folders(&space.id, false).await?;
            let lists = self.client.folderless_lists(&space.id, false).await?;
            let to_h = |l: List| HList {
                id: l.id,
                name: l.name,
            };
            tree.spaces.push(HSpace {
                id: space.id,
                name: space.name,
                folders: folders
                    .into_iter()
                    .map(|f| HFolder {
                        id: f.id,
                        name: f.name,
                        lists: f.lists.into_iter().map(to_h).collect(),
                    })
                    .collect(),
                lists: lists.into_iter().map(to_h).collect(),
            });
        }
        self.cache.save("hierarchy.json", &tree);
        *self.hierarchy.lock().expect("hierarchy lock") = Some((tree.clone(), true));
        Ok((tree, true))
    }

    /// Tree from memory or disk, without touching the network.
    fn cached_tree(&self) -> Option<Hierarchy> {
        if let Some((h, _)) = self.hierarchy.lock().expect("hierarchy lock").clone() {
            return Some(h);
        }
        if self.refresh {
            return None;
        }
        self.cache.load::<Hierarchy>("hierarchy.json")
    }

    /// Run `find` against the tree; on a miss with a cached tree, refetch once.
    async fn with_tree<T>(
        &self,
        find: impl Fn(&Hierarchy) -> Result<Option<T>>,
    ) -> Result<(Option<T>, Hierarchy)> {
        let (tree, fresh) = self.hierarchy_with_origin(false).await?;
        if let Some(hit) = find(&tree)? {
            return Ok((Some(hit), tree));
        }
        if fresh {
            return Ok((None, tree));
        }
        let (tree, _) = self.hierarchy_with_origin(true).await?;
        Ok((find(&tree)?, tree))
    }

    /// id | alias | `Space/Folder/List` | `Space/List` | unique list name.
    pub async fn list(&self, input: &str) -> Result<ListRef> {
        let input = input.trim();
        let target = self
            .aliases
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(input))
            .map(|(_, v)| v.as_str())
            .unwrap_or(input);
        let is_id = target.chars().all(|c| c.is_ascii_digit()) && !target.is_empty();
        if is_id {
            // Ids never trigger a hierarchy walk; the cached tree (if any)
            // only supplies the display name.
            return Ok(self
                .cached_tree()
                .and_then(|h| h.lists().into_iter().find(|l| l.id == target))
                .unwrap_or_else(|| ListRef {
                    id: target.to_string(),
                    name: target.to_string(),
                    space: None,
                    folder: None,
                }));
        }
        let lower = target.to_lowercase();
        let (hit, tree) = self
            .with_tree(|h| {
                let lists = h.lists();
                if target.contains('/') {
                    return unique("list", target, &lists, ListRef::path, |l| {
                        let p = l.path().to_lowercase();
                        p == lower || p.ends_with(&format!("/{lower}"))
                    });
                }
                unique("list", target, &lists, ListRef::path, |l| {
                    l.name.to_lowercase() == lower
                })
            })
            .await?;
        if let Some(l) = hit {
            return Ok(l);
        }
        let lists = tree.lists();
        let mut names: Vec<String> = lists.iter().map(|l| l.name.clone()).collect();
        names.extend(lists.iter().map(ListRef::path));
        Err(not_found("list", input, &names))
    }

    pub async fn space(&self, input: &str) -> Result<SpaceRef> {
        let input = input.trim();
        let lower = input.to_lowercase();
        let (hit, tree) = self
            .with_tree(|h| {
                let spaces: Vec<SpaceRef> = h
                    .spaces
                    .iter()
                    .map(|s| SpaceRef {
                        id: s.id.clone(),
                        name: s.name.clone(),
                    })
                    .collect();
                unique(
                    "space",
                    input,
                    &spaces,
                    |s| s.name.clone(),
                    |s| s.id == input || s.name.to_lowercase() == lower,
                )
            })
            .await?;
        hit.ok_or_else(|| {
            let names: Vec<String> = tree.spaces.iter().map(|s| s.name.clone()).collect();
            not_found("space", input, &names)
        })
    }

    /// id | `Space/Folder` | unique folder name.
    pub async fn folder(&self, input: &str) -> Result<FolderRef> {
        let input = input.trim();
        let lower = input.to_lowercase();
        let label = |f: &FolderRef| format!("{}/{}", f.space, f.name);
        let (hit, tree) = self
            .with_tree(|h| {
                unique("folder", input, &h.folders(), label, |f| {
                    f.id == input
                        || f.name.to_lowercase() == lower
                        || label(f).to_lowercase() == lower
                })
            })
            .await?;
        hit.ok_or_else(|| {
            let names: Vec<String> = tree.folders().iter().map(|f| f.name.clone()).collect();
            not_found("folder", input, &names)
        })
    }

    /// `GET /v2/list/{id}` with caching (statuses rarely change).
    pub async fn list_detail(&self, list_id: &str) -> Result<List> {
        let name = format!("lists/{list_id}.json");
        if !self.refresh {
            if let Some(l) = self.cache.load::<List>(&name) {
                return Ok(l);
            }
        }
        let l = self.client.list(list_id).await?;
        self.cache.save(&name, &l);
        Ok(l)
    }

    /// Canonical status spelling for a list (case-insensitive match).
    pub async fn status(&self, list_id: &str, input: &str) -> Result<String> {
        let mut list = self.list_detail(list_id).await?;
        let find = |l: &List| {
            l.statuses
                .iter()
                .find(|s| s.status.eq_ignore_ascii_case(input.trim()))
                .map(|s| s.status.clone())
        };
        if let Some(s) = find(&list) {
            return Ok(s);
        }
        if !self.refresh {
            list = self.client.list(list_id).await?;
            self.cache.save(&format!("lists/{list_id}.json"), &list);
            if let Some(s) = find(&list) {
                return Ok(s);
            }
        }
        let names: Vec<String> = list.statuses.iter().map(|s| s.status.clone()).collect();
        let mut suggestions = suggest(input, &names);
        if suggestions.is_empty() {
            suggestions = names;
        }
        Err(ClickUpError::resolve("status", input, suggestions))
    }

    pub async fn members(&self) -> Result<Vec<User>> {
        if !self.refresh {
            if let Some(m) = self.cache.load::<Vec<User>>("members.json") {
                return Ok(m);
            }
        }
        let m = self.client.workspace_members().await?;
        self.cache.save("members.json", &m);
        Ok(m)
    }

    /// `me` | id | email | username (exact, then unique partial match).
    pub async fn member(&self, input: &str) -> Result<User> {
        let input = input.trim();
        if input.eq_ignore_ascii_case("me") {
            return self.client.user().await;
        }
        let lower = input.to_lowercase();
        let members = self.members().await?;
        let label = |u: &User| match &u.email {
            Some(e) => format!("{} <{e}>", u.display()),
            None => u.display(),
        };
        let exact = |u: &User| {
            u.id == input
                || u.email
                    .as_deref()
                    .is_some_and(|e| e.eq_ignore_ascii_case(input))
                || u.username
                    .as_deref()
                    .is_some_and(|n| n.to_lowercase() == lower)
        };
        if let Some(u) = unique("member", input, &members, label, exact)? {
            return Ok(u);
        }
        let partial = |u: &User| {
            u.username
                .as_deref()
                .is_some_and(|n| n.to_lowercase().contains(&lower))
                || u.email
                    .as_deref()
                    .is_some_and(|e| e.to_lowercase().contains(&lower))
        };
        if let Some(u) = unique("member", input, &members, label, partial)? {
            return Ok(u);
        }
        if input.chars().all(|c| c.is_ascii_digit()) && !input.is_empty() {
            return Ok(User {
                id: input.to_string(),
                ..Default::default()
            });
        }
        let names: Vec<String> = members.iter().filter_map(|u| u.username.clone()).collect();
        Err(not_found("member", input, &names))
    }

    /// Numeric user id for API bodies.
    pub async fn member_id(&self, input: &str) -> Result<i64> {
        let u = self.member(input).await?;
        u.id.parse()
            .map_err(|_| ClickUpError::Invalid(format!("member '{input}' has a non-numeric id")))
    }

    pub async fn fields(&self, list_id: &str) -> Result<Vec<CustomField>> {
        let name = format!("fields/{list_id}.json");
        if !self.refresh {
            if let Some(f) = self.cache.load::<Vec<CustomField>>(&name) {
                return Ok(f);
            }
        }
        let f = self.client.list_fields(list_id).await?;
        self.cache.save(&name, &f);
        Ok(f)
    }

    /// Field by id or (case-insensitive) name within a list.
    pub async fn field(&self, list_id: &str, input: &str) -> Result<CustomField> {
        let input = input.trim();
        let find = |fields: &[CustomField]| {
            fields
                .iter()
                .find(|f| f.id == input || f.name.eq_ignore_ascii_case(input))
                .cloned()
        };
        let fields = self.fields(list_id).await?;
        if let Some(f) = find(&fields) {
            return Ok(f);
        }
        let fields = self.client.list_fields(list_id).await?;
        self.cache.save(&format!("fields/{list_id}.json"), &fields);
        find(&fields).ok_or_else(|| {
            let names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
            let mut suggestions = suggest(input, &names);
            if suggestions.is_empty() {
                suggestions = names;
            }
            ClickUpError::resolve("field", input, suggestions)
        })
    }

    /// Human input → API value, resolving members / tasks where needed.
    /// For `users` / `tasks` fields, prefix an item with `-` to remove it.
    pub async fn field_value(&self, field: &CustomField, raw: &str) -> Result<FieldValue> {
        match field.kind.as_str() {
            "users" | "tasks" | "list_relationship" => {
                let (mut add, mut rem) = (Vec::new(), Vec::new());
                for item in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    let (target, bucket) = match item.strip_prefix('-') {
                        Some(rest) => (rest.trim(), &mut rem),
                        None => (item, &mut add),
                    };
                    let id = if field.kind == "users" {
                        json!(self.member_id(target).await?)
                    } else {
                        let r: TaskRef = target.parse()?;
                        json!(self.client.task_id(&r).await?)
                    };
                    bucket.push(id);
                }
                Ok(relation_value(add, rem))
            }
            _ => convert_field_value(field, raw, self.client.timezone()),
        }
    }

    /// `task` → 0, `milestone` → 1, numeric id, or a custom task type name.
    pub async fn task_type(&self, input: &str) -> Result<i64> {
        let input = input.trim();
        match input.to_ascii_lowercase().as_str() {
            "task" => return Ok(0),
            "milestone" => return Ok(1),
            _ => {}
        }
        if let Ok(n) = input.parse() {
            return Ok(n);
        }
        let types = self.client.custom_task_types().await?;
        let name_of = |t: &Value| {
            t.get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        if let Some(t) = types
            .iter()
            .find(|t| name_of(t).eq_ignore_ascii_case(input))
        {
            if let Some(id) = t.get("id").and_then(Value::as_i64) {
                return Ok(id);
            }
        }
        let names: Vec<String> = types.iter().map(name_of).collect();
        Err(not_found("task type", input, &names))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestions_are_ranked_and_capped() {
        let names: Vec<String> = ["Sprint 12", "Sprint 13", "Backlog", "Sprint 1", "Bugs"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let s = suggest("sprnt 12", &names);
        assert_eq!(s.first().map(String::as_str), Some("Sprint 12"));
        assert!(s.len() <= 3);
        assert!(suggest("zzzz", &names).is_empty());
    }

    #[test]
    fn list_paths() {
        let l = ListRef {
            id: "1".into(),
            name: "Sprint".into(),
            space: Some("Eng".into()),
            folder: Some("Q4".into()),
        };
        assert_eq!(l.path(), "Eng/Q4/Sprint");
    }
}
