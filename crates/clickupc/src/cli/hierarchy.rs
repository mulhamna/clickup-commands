//! tree, space, folder, list, alias, cache, member.

use anyhow::{bail, Result};
use clap::Subcommand;
use clickupc_core::{config::ConfigFile, resolve::suggest};
use serde_json::json;

use super::Ctx;
use crate::output::{print_json, print_table};

#[derive(Debug, Subcommand)]
pub enum SpaceCmd {
    List {
        #[arg(long)]
        archived: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum FolderCmd {
    List {
        #[arg(long)]
        space: String,
    },
    /// Create a folder in a space
    Create {
        #[arg(long)]
        space: String,
        #[arg(long)]
        name: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ListCmd {
    /// Lists, optionally within a space or folder
    List {
        #[arg(long)]
        space: Option<String>,
        #[arg(long)]
        folder: Option<String>,
    },
    /// Statuses, custom fields and members of a list
    View { list: String },
    /// Create a list in a folder, or a folderless list in a space
    Create {
        #[arg(long, conflicts_with = "space")]
        folder: Option<String>,
        #[arg(long)]
        space: Option<String>,
        #[arg(long)]
        name: String,
        /// Markdown description
        #[arg(long)]
        content: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum AliasCmd {
    /// Map a short name to a list (stored in the profile)
    Set {
        name: String,
        list: String,
    },
    List,
    Remove {
        name: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum CacheCmd {
    /// Delete cached hierarchy, members and fields for the workspace
    Clear,
}

#[derive(Debug, Subcommand)]
pub enum MemberCmd {
    List {
        #[arg(long)]
        list: Option<String>,
    },
    /// Fuzzy-find a member by username or email
    Find { query: String },
}

pub async fn tree(ctx: &Ctx, space: Option<String>, archived: bool) -> Result<()> {
    let s = ctx.session()?;
    let r = s.resolver()?;
    let mut tree = if archived {
        // Archived items are never cached; walk directly.
        let mut t = clickupc_core::resolve::Hierarchy::default();
        for sp in s.client.spaces(true).await? {
            let folders = s.client.folders(&sp.id, true).await?;
            let lists = s.client.folderless_lists(&sp.id, true).await?;
            t.spaces.push(clickupc_core::resolve::HSpace {
                id: sp.id,
                name: sp.name,
                folders: folders
                    .into_iter()
                    .map(|f| clickupc_core::resolve::HFolder {
                        id: f.id,
                        name: f.name,
                        lists: f
                            .lists
                            .into_iter()
                            .map(|l| clickupc_core::resolve::HList {
                                id: l.id,
                                name: l.name,
                            })
                            .collect(),
                    })
                    .collect(),
                lists: lists
                    .into_iter()
                    .map(|l| clickupc_core::resolve::HList {
                        id: l.id,
                        name: l.name,
                    })
                    .collect(),
            });
        }
        t
    } else {
        r.hierarchy(ctx.refresh).await?
    };
    if let Some(sp) = space {
        let sp = r.space(&sp).await?;
        tree.spaces.retain(|x| x.id == sp.id);
    }
    if ctx.json {
        return print_json(&tree);
    }
    for sp in &tree.spaces {
        println!("{}  ({})", sp.name, sp.id);
        let n = sp.folders.len() + sp.lists.len();
        let mut i = 0;
        for f in &sp.folders {
            i += 1;
            let last = i == n;
            println!(
                "{} {}/  ({})",
                if last { "└──" } else { "├──" },
                f.name,
                f.id
            );
            for (j, l) in f.lists.iter().enumerate() {
                let bar = if last { "    " } else { "│   " };
                let br = if j + 1 == f.lists.len() {
                    "└──"
                } else {
                    "├──"
                };
                println!("{bar}{br} {}  ({})", l.name, l.id);
            }
        }
        for l in &sp.lists {
            i += 1;
            println!(
                "{} {}  ({})",
                if i == n { "└──" } else { "├──" },
                l.name,
                l.id
            );
        }
    }
    Ok(())
}

pub async fn space(ctx: &Ctx, cmd: SpaceCmd) -> Result<()> {
    let s = ctx.session()?;
    let SpaceCmd::List { archived } = cmd;
    let spaces = s.client.spaces(archived).await?;
    if ctx.json {
        return print_json(&spaces);
    }
    print_table(
        &["ID", "Name", "Private"],
        spaces.iter().map(|x| {
            vec![
                x.id.clone(),
                x.name.clone(),
                if x.private { "yes" } else { "" }.to_string(),
            ]
        }),
    );
    Ok(())
}

pub async fn folder(ctx: &Ctx, cmd: FolderCmd) -> Result<()> {
    let s = ctx.session()?;
    let r = s.resolver()?;
    match cmd {
        FolderCmd::List { space } => {
            let sp = r.space(&space).await?;
            let folders = s.client.folders(&sp.id, false).await?;
            if ctx.json {
                return print_json(&folders);
            }
            print_table(
                &["ID", "Name", "Lists"],
                folders
                    .iter()
                    .map(|f| vec![f.id.clone(), f.name.clone(), f.lists.len().to_string()]),
            );
        }
        FolderCmd::Create { space, name } => {
            let sp = r.space(&space).await?;
            let f = s.client.create_folder(&sp.id, &name).await?;
            r.cache().clear()?;
            if ctx.json {
                return print_json(&f);
            }
            println!("Created folder '{}' ({}) in {}.", f.name, f.id, sp.name);
        }
    }
    Ok(())
}

pub async fn list(ctx: &Ctx, cmd: ListCmd) -> Result<()> {
    let s = ctx.session()?;
    let r = s.resolver()?;
    match cmd {
        ListCmd::List { space, folder } => {
            let mut lists = r.hierarchy(ctx.refresh).await?.lists();
            if let Some(f) = folder {
                let f = r.folder(&f).await?;
                lists.retain(|l| {
                    l.folder.as_deref() == Some(f.name.as_str())
                        && l.space.as_deref() == Some(f.space.as_str())
                });
            }
            if let Some(sp) = space {
                let sp = r.space(&sp).await?;
                lists.retain(|l| l.space.as_deref() == Some(sp.name.as_str()));
            }
            if ctx.json {
                return print_json(&lists);
            }
            print_table(
                &["ID", "Path"],
                lists.iter().map(|l| vec![l.id.clone(), l.path()]),
            );
        }
        ListCmd::View { list } => {
            let l = r.list(&list).await?;
            let detail = s.client.list(&l.id).await?;
            let fields = r.fields(&l.id).await?;
            let members = s.client.list_members(&l.id).await?;
            if ctx.json {
                return print_json(&json!({
                    "list": detail, "fields": fields, "members": members,
                }));
            }
            println!("{}  ({})", l.path(), l.id);
            println!("\nStatuses");
            print_table(
                &["Status", "Type"],
                detail
                    .statuses
                    .iter()
                    .map(|st| vec![st.status.clone(), st.kind.clone().unwrap_or_default()]),
            );
            if !fields.is_empty() {
                println!("\nCustom fields");
                print_table(
                    &["Name", "Type", "Options"],
                    fields.iter().map(|f| {
                        vec![
                            f.name.clone(),
                            f.kind.clone(),
                            f.options()
                                .iter()
                                .map(|o| o.name.clone())
                                .collect::<Vec<_>>()
                                .join(", "),
                        ]
                    }),
                );
            }
            if !members.is_empty() {
                println!("\nMembers");
                print_table(
                    &["ID", "Username", "Email"],
                    members.iter().map(|u| {
                        vec![
                            u.id.clone(),
                            u.username.clone().unwrap_or_default(),
                            u.email.clone().unwrap_or_default(),
                        ]
                    }),
                );
            }
        }
        ListCmd::Create {
            folder,
            space,
            name,
            content,
        } => {
            let (f, sp) = match (folder, space) {
                (Some(f), _) => (Some(r.folder(&f).await?.id), None),
                (None, Some(sp)) => (None, Some(r.space(&sp).await?.id)),
                (None, None) => bail!("pass --folder or --space"),
            };
            let l = s
                .client
                .create_list(f.as_deref(), sp.as_deref(), &name, content.as_deref())
                .await?;
            r.cache().clear()?;
            if ctx.json {
                return print_json(&l);
            }
            println!("Created list '{}' ({}).", l.name, l.id);
        }
    }
    Ok(())
}

pub async fn alias(ctx: &Ctx, cmd: AliasCmd) -> Result<()> {
    let mut file = ConfigFile::load()?;
    let name = ctx
        .profile
        .clone()
        .unwrap_or_else(|| file.selected_profile_name());
    match cmd {
        AliasCmd::Set { name: alias, list } => {
            let s = ctx.session()?;
            let l = s.resolver()?.list(&list).await?;
            file.profile_mut(&name)
                .aliases
                .insert(alias.clone(), l.id.clone());
            file.save()?;
            println!("Alias '{alias}' → {} ({}).", l.path(), l.id);
        }
        AliasCmd::List => {
            let aliases = file
                .profiles
                .get(&name)
                .map(|p| p.aliases.clone())
                .unwrap_or_default();
            if ctx.json {
                return print_json(&aliases);
            }
            print_table(
                &["Alias", "List"],
                aliases.iter().map(|(k, v)| vec![k.clone(), v.clone()]),
            );
        }
        AliasCmd::Remove { name: alias } => {
            if file.profile_mut(&name).aliases.remove(&alias).is_none() {
                bail!("no alias '{alias}' in profile '{name}'");
            }
            file.save()?;
            println!("Removed alias '{alias}'.");
        }
    }
    Ok(())
}

pub fn cache(ctx: &Ctx, cmd: CacheCmd) -> Result<()> {
    let CacheCmd::Clear = cmd;
    let s = ctx.session()?;
    let r = s.resolver()?;
    r.cache().clear()?;
    println!("Cleared {}.", r.cache().dir().display());
    Ok(())
}

pub async fn member(ctx: &Ctx, cmd: MemberCmd) -> Result<()> {
    let s = ctx.session()?;
    let r = s.resolver()?;
    let members = match &cmd {
        MemberCmd::List { list: Some(l) } => s.client.list_members(&r.list(l).await?.id).await?,
        _ => r.members().await?,
    };
    let members = match &cmd {
        MemberCmd::Find { query } => {
            let q = query.to_lowercase();
            let mut hits: Vec<_> = members
                .iter()
                .filter(|u| {
                    u.username
                        .as_deref()
                        .is_some_and(|n| n.to_lowercase().contains(&q))
                        || u.email
                            .as_deref()
                            .is_some_and(|e| e.to_lowercase().contains(&q))
                })
                .cloned()
                .collect();
            if hits.is_empty() {
                let names: Vec<String> =
                    members.iter().filter_map(|u| u.username.clone()).collect();
                let close = suggest(query, &names);
                hits = members
                    .iter()
                    .filter(|u| u.username.as_ref().is_some_and(|n| close.contains(n)))
                    .cloned()
                    .collect();
            }
            hits
        }
        MemberCmd::List { .. } => members,
    };
    if ctx.json {
        return print_json(&members);
    }
    print_table(
        &["ID", "Username", "Email"],
        members.iter().map(|u| {
            vec![
                u.id.clone(),
                u.username.clone().unwrap_or_default(),
                u.email.clone().unwrap_or_default(),
            ]
        }),
    );
    Ok(())
}
