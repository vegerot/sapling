/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! Mercurial-specific config postprocessing

use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::fs;
#[cfg(feature = "fb")]
use std::fs::read_to_string;
use std::hash::Hash;
use std::io;
use std::io::Error as IOError;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use configmodel::Config;
use configmodel::ConfigExt;
use gitcompat::init::translated_git_repo_config_path;
use gitcompat::init::translated_git_user_config_path;
use hgplain;
use identity::Identity;
use minibytes::Text;
use repo_minimal_info::RepoMinimalInfo;
use url::Url;

use crate::config::ConfigSet;
use crate::config::Options;
use crate::error::Error;
use crate::error::Errors;
#[cfg(feature = "fb")]
use crate::fb::FbConfigMode;

pub trait OptionsHgExt {
    /// Drop configs according to `$HGPLAIN` and `$HGPLAINEXCEPT`.
    fn process_hgplain(self) -> Self;

    /// Set section remap. If a section name matches an entry key, it will be treated as if the
    /// name is the entry value. The remap wouldn't happen recursively. For example, with a
    /// `{"A": "B", "B": "C"}` map, section name "A" will be treated as "B", not "C".
    /// This is implemented via `append_filter`.
    fn remap_sections<K: Eq + Hash + Into<Text>, V: Into<Text>>(self, remap: HashMap<K, V>)
    -> Self;

    /// Filter sections. Sections outside include_sections won't be loaded.
    /// This is implemented via `append_filter`.
    fn filter_sections<B: Clone + Into<Text>>(self, include_sections: Vec<B>) -> Self;
}

pub trait ConfigSetHgExt {
    fn load(&mut self, info: Option<&RepoMinimalInfo>, opts: Options) -> Result<(), Errors>;

    /// Load system config files if config environment variable is not set.
    /// Return errors parsing files.
    fn load_system(&mut self, opts: Options, identity: &Identity) -> Vec<Error>;

    /// Optionally refresh the dynamic config in the background.
    fn maybe_refresh_dynamic(
        &self,
        info: Option<&RepoMinimalInfo>,
        identity: &Identity,
    ) -> Result<()>;

    /// Load user config files (and environment variables).  If config environment variable is
    /// set, load files listed in that environment variable instead.
    /// Return errors parsing files.
    fn load_user(&mut self, opts: Options, identity: &Identity) -> Vec<Error>;

    /// Load repo config files.
    fn load_repo(&mut self, info: &RepoMinimalInfo, opts: Options) -> Vec<Error>;

    /// Load a specified config file. Respect HGPLAIN environment variables.
    /// Return errors parsing files.
    fn load_hgrc(&mut self, path: impl AsRef<Path>, source: &'static str) -> Vec<Error>;
}

/// Load config from specified "minimal repo", or global config if no path specified.
/// `extra_values` contains config overrides (i.e. "--config" CLI values).
/// `extra_files` contains additional config files (i.e. "--configfile" CLI values).
pub fn load(info: Option<&RepoMinimalInfo>, pinned: &[PinnedConfig]) -> Result<ConfigSet> {
    load_with_options(info, pinned, Options::default())
}

/// Like `load`, but intended to be used by applications that embed Sapling libraries.
/// In particular, defer to the system "sl" binary to refresh dynamic config.
pub fn embedded_load(info: Option<&RepoMinimalInfo>, pinned: &[PinnedConfig]) -> Result<ConfigSet> {
    let mut opts: Options = Default::default();
    opts.minimize_dynamic_gen = true;
    load_with_options(info, pinned, opts)
}

fn load_with_options(
    info: Option<&RepoMinimalInfo>,
    pinned: &[PinnedConfig],
    opts: Options,
) -> Result<ConfigSet> {
    let mut cfg = ConfigSet::new().named("root");
    let mut errors = Vec::new();

    tracing::debug!(?pinned, repo_path=?info.map(|i| &i.path));

    // "--configfile" and "--config" values are loaded as "pinned". This lets us load them
    // first so they can inform further config loading, but also make sure they still take
    // precedence over "regular" configs.
    set_pinned_with_errors(&mut cfg, pinned, &mut errors);

    match cfg.load(info, opts) {
        Ok(_) => {
            if !errors.is_empty() {
                return Err(Errors(errors).into());
            }
        }
        Err(mut err) => {
            err.0.extend(errors);
            return Err(err.into());
        }
    }

    Ok(cfg)
}

pub fn set_pinned(cfg: &mut ConfigSet, pinned: &[PinnedConfig]) -> Result<()> {
    let mut errors = Vec::new();
    set_pinned_with_errors(cfg, pinned, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(Errors(errors).into())
    }
}

fn set_pinned_with_errors(cfg: &mut ConfigSet, pinned: &[PinnedConfig], errors: &mut Vec<Error>) {
    for pinned in pinned {
        let opts = Options::default().pin(true);

        match pinned {
            PinnedConfig::Raw(raw, source) => {
                if let Err(err) = set_override(cfg, raw, opts.clone().source(source.clone())) {
                    errors.push(err);
                }
            }
            PinnedConfig::KeyValue(section, name, value, source) => cfg.set(
                section,
                name,
                Some(value),
                &opts.clone().source(source.clone()),
            ),
            PinnedConfig::File(path, source) => {
                errors.extend(cfg.load_path(path.as_ref(), &opts.clone().source(source.clone())));
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum PinnedConfig {
    // ("foo.bar=baz", <source>)
    Raw(Text, Text),
    // ("foo", "bar", "baz", <source>)
    KeyValue(Text, Text, Text, Text),
    // ("some/file.rc", <source>)
    File(Text, Text),
}

impl PinnedConfig {
    pub fn from_cli_opts(config: &[String], configfile: &[String]) -> Vec<Self> {
        // "--config" comes last so they take precedence
        configfile
            .iter()
            .map(|f| PinnedConfig::File(f.to_string().into(), "--configfile".into()))
            .chain(
                config
                    .iter()
                    .map(|c| PinnedConfig::Raw(c.to_string().into(), "--config".into())),
            )
            .collect()
    }
}

impl OptionsHgExt for Options {
    fn process_hgplain(self) -> Self {
        if hgplain::is_plain(None) {
            let (section_exclude_list, ui_exclude_list) = {
                let plain_exceptions = hgplain::exceptions();

                // [defaults] and [commands] are always excluded.
                let mut section_exclude_list: HashSet<Text> =
                    ["defaults", "commands"].iter().map(|&s| s.into()).collect();

                // [alias], [revsetalias], [templatealias] are excluded if they are outside
                // HGPLAINEXCEPT.
                for name in ["alias", "revsetalias", "templatealias"] {
                    if !plain_exceptions.contains(name) {
                        section_exclude_list.insert(Text::from(name));
                    }
                }

                // These configs under [ui] are always excluded.
                let mut ui_exclude_list: HashSet<Text> = [
                    "debug",
                    "fallbackencoding",
                    "quiet",
                    "slash",
                    "logtemplate",
                    "statuscopies",
                    "style",
                    "traceback",
                    "verbose",
                ]
                .iter()
                .map(|&s| s.into())
                .collect();
                // exitcodemask is excluded if exitcode is outside HGPLAINEXCEPT.
                if !plain_exceptions.contains("exitcode") {
                    ui_exclude_list.insert("exitcodemask".into());
                }

                (section_exclude_list, ui_exclude_list)
            };

            let filter = move |section: Text, name: Text, value: Option<Text>| {
                if section_exclude_list.contains(&section)
                    || (section.as_ref() == "ui" && ui_exclude_list.contains(&name))
                {
                    None
                } else {
                    Some((section, name, value))
                }
            };

            self.append_filter(Box::new(filter))
        } else {
            self
        }
    }

    /// Filter sections. Sections outside of include_sections won't be loaded.
    /// This is implemented via `append_filter`.
    fn filter_sections<B: Clone + Into<Text>>(self, include_sections: Vec<B>) -> Self {
        let include_list: HashSet<Text> = include_sections
            .iter()
            .cloned()
            .map(|section| section.into())
            .collect();

        let filter = move |section: Text, name: Text, value: Option<Text>| {
            if include_list.contains(&section) {
                Some((section, name, value))
            } else {
                None
            }
        };

        self.append_filter(Box::new(filter))
    }

    /// Set section remap. If a section name matches an entry key, it will be treated as if the
    /// name is the entry value. The remap wouldn't happen recursively. For example, with a
    /// `{"A": "B", "B": "C"}` map, section name "A" will be treated as "B", not "C".
    /// This is implemented via `append_filter`.
    fn remap_sections<K, V>(self, remap: HashMap<K, V>) -> Self
    where
        K: Eq + Hash + Into<Text>,
        V: Into<Text>,
    {
        let remap: HashMap<Text, Text> = remap
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();

        let filter = move |section: Text, name: Text, value: Option<Text>| {
            let section = remap.get(&section).cloned().unwrap_or(section);
            Some((section, name, value))
        };

        self.append_filter(Box::new(filter))
    }
}

/// override config values from a list of --config overrides
fn set_override(config: &mut ConfigSet, raw: &Text, opts: Options) -> crate::Result<()> {
    let equals_pos = raw
        .as_ref()
        .find('=')
        .ok_or_else(|| Error::ParseFlag(raw.to_string()))?;
    let section_name_pair = &raw[..equals_pos];
    let value = &raw[equals_pos + 1..];

    let dot_pos = section_name_pair
        .find('.')
        .ok_or_else(|| Error::ParseFlag(raw.to_string()))?;
    let section = &section_name_pair[..dot_pos];
    let name = &section_name_pair[dot_pos + 1..];

    config.set(section, name, Some(value), &opts);

    Ok(())
}

impl ConfigSetHgExt for ConfigSet {
    /// Load system, user config files.
    fn load(&mut self, info: Option<&RepoMinimalInfo>, opts: Options) -> Result<(), Errors> {
        tracing::info!(repo_path=?info.map(|i| &i.path), "loading config");

        self.clear_unpinned();

        let ident = match info {
            None => identity::default(),
            Some(i) => i.ident,
        };

        // The ".git/sl" path for a dotgit repo. Otherwise None.
        let dotgit_sl_path = match info {
            None => None,
            Some(info) => {
                if info.ident.dot_dir().starts_with(".git") {
                    Some(&info.dot_hg_path)
                } else {
                    None
                }
            }
        };

        let mut errors = vec![];

        // Don't pin any configs we load. We are doing the "default" config loading, which
        // should be cleared if we load() again (via clear_unpinned());
        let opts = opts.pin(false);

        // The config priority from low to high is:
        //
        //   builtin
        //   dynamic
        //   system
        //   user-git (only for dotgit repos)
        //   user
        //   repo-git (only for dotgit repos)
        //   repo
        //
        // We load things out of order a bit since the dynamic config can depend
        // on system config (namely, auth_proxy.unix_socket_path).

        let mut layers = crate::builtin_static::builtin_system(opts.clone(), &ident, info);

        let dynamic_layer_idx = layers.len();

        let mut system = ConfigSet::new().named("system");
        errors.append(&mut system.load_system(opts.clone(), &ident));
        layers.push(Arc::new(system));

        if let Some(dotgit_sl_path) = dotgit_sl_path {
            let mut user_git = ConfigSet::new().named("user-git");
            let path = translated_git_user_config_path(dotgit_sl_path, ident);
            errors.append(&mut user_git.load_hgrc(path, "user-git"));
            layers.push(Arc::new(user_git));
        }

        let mut user = ConfigSet::new().named("user");
        errors.append(&mut user.load_user(opts.clone(), &ident));
        layers.push(Arc::new(user));

        if let Some(info) = info {
            if let Some(dotgit_sl_path) = dotgit_sl_path {
                let mut repo_git = ConfigSet::new().named("repo-git");
                let path = translated_git_repo_config_path(dotgit_sl_path, ident);
                errors.append(&mut repo_git.load_hgrc(path, "repo-git"));
                layers.push(Arc::new(repo_git));
            }
            let mut local = ConfigSet::new().named("repo");
            errors.append(&mut local.load_repo(info, opts.clone()));
            layers.push(Arc::new(local));
            if let Err(e) = read_set_repo_name(&layers, self, &info.dot_hg_path) {
                errors.push(e);
            }
        }

        #[cfg(feature = "fb")]
        {
            let dynamic = load_dynamic(
                info,
                opts,
                &ident,
                layers
                    .get_opt("auth_proxy", "unix_socket_path")
                    .unwrap_or_default(),
                &mut errors,
            )
            .map_err(|e| Errors(vec![Error::Other(e)]))?;
            layers.insert(dynamic_layer_idx, Arc::new(dynamic));
        }

        self.secondary(Arc::new(layers));

        // Wait until config is fully loaded so maybe_refresh_dynamic() itself sees
        // correct config values.
        self.maybe_refresh_dynamic(info, &ident)
            .map_err(|e| Errors(vec![Error::Other(e)]))?;

        if !errors.is_empty() {
            return Err(Errors(errors));
        }

        Ok(())
    }

    fn load_system(&mut self, opts: Options, ident: &Identity) -> Vec<Error> {
        let opts = opts.source("system").process_hgplain();
        let mut errors = Vec::new();

        for system_path in ident.system_config_paths() {
            if system_path.exists() {
                errors.append(&mut self.load_path(system_path, &opts));
            }
        }

        errors
    }

    #[cfg(feature = "fb")]
    fn maybe_refresh_dynamic(
        &self,
        info: Option<&RepoMinimalInfo>,
        identity: &Identity,
    ) -> Result<()> {
        use std::process::Command;
        use std::time::Duration;
        use std::time::SystemTime;

        use spawn_ext::CommandExt;

        let mode = FbConfigMode::from_identity(identity);
        if !mode.need_dynamic_generator() {
            return Ok(());
        }

        let dynamic_path = get_config_dir(info)?.join("hgrc.dynamic");

        // Regenerate if mtime is old.
        let generation_time: Option<u64> = self.get_opt("configs", "generationtime")?;
        let recursion_marker = env::var("HG_INTERNALCONFIG_IS_REFRESHING");
        let mut skip_reason = None;

        if recursion_marker.is_err() {
            if let Some(generation_time) = generation_time {
                let generation_time = Duration::from_secs(generation_time);
                let mtime_age = SystemTime::now()
                    .duration_since(dynamic_path.metadata()?.modified()?)
                    // An error from duration_since means 'now' is older than
                    // 'last_modified'. In that case, let's assume the file
                    // is brand new and has an age of 0.
                    .unwrap_or(Duration::from_secs(0));
                if mtime_age > generation_time {
                    let config_regen_command: Vec<String> =
                        self.get_or("configs", "regen-command", || {
                            vec![
                                identity::cli_name().to_string(),
                                "debugrefreshconfig".to_string(),
                            ]
                        })?;
                    tracing::debug!(
                        "spawn {:?} because mtime({}) {:?} > generation_time {:?}",
                        &config_regen_command,
                        dynamic_path.display(),
                        mtime_age,
                        generation_time
                    );
                    if !config_regen_command.is_empty() {
                        let mut command = Command::new(&config_regen_command[0]);
                        command
                            .args(&config_regen_command[1..])
                            .env("HG_INTERNALCONFIG_IS_REFRESHING", "1");

                        if let Some(info) = info {
                            command.current_dir(&info.dot_hg_path);
                        }

                        let _ = command.spawn_detached();
                    }
                } else {
                    skip_reason = Some("mtime <= configs.generationtime");
                }
            } else {
                skip_reason = Some("configs.generationtime is not set");
            }
        } else {
            skip_reason = Some("HG_INTERNALCONFIG_IS_REFRESHING is set");
        }
        if let Some(reason) = skip_reason {
            tracing::debug!("skip spawning debugrefreshconfig because {}", reason);
        }

        Ok(())
    }

    #[cfg(not(feature = "fb"))]
    fn maybe_refresh_dynamic(
        &self,
        _info: Option<&RepoMinimalInfo>,
        _identity: &Identity,
    ) -> Result<()> {
        Ok(())
    }

    fn load_user(&mut self, opts: Options, ident: &Identity) -> Vec<Error> {
        let path = ident.user_config_path();
        self.load_user_internal(path.as_ref(), opts)
    }

    fn load_repo(&mut self, info: &RepoMinimalInfo, opts: Options) -> Vec<Error> {
        let mut errors = Vec::new();

        let opts = opts.source("repo").process_hgplain();

        let repo_config_path = info.dot_hg_path.join(info.ident.config_repo_file());
        errors.append(&mut self.load_path(repo_config_path, &opts));

        errors
    }

    fn load_hgrc(&mut self, path: impl AsRef<Path>, source: &'static str) -> Vec<Error> {
        let opts = Options::new().source(source).process_hgplain();
        self.load_path(path, &opts)
    }
}

/// Read repo name from various places (remotefilelog.reponame, paths.default, .hg/reponame).
///
/// Try to write the reponame back to `.hg/reponame`, and set `remotefilelog.reponame`
/// for code paths using them.
///
/// If `configs.forbid-empty-reponame` is `true`, raise if the repo name is empty
/// and `paths.default` is set.
fn read_set_repo_name(
    input_config: &dyn Config,
    output_config: &mut ConfigSet,
    repo_path: &Path,
) -> crate::Result<String> {
    let (repo_name, source): (String, &str) = {
        let mut name: String = input_config.get_or_default("remotefilelog", "reponame")?;
        let mut source = "remotefilelog.reponame";
        if name.is_empty() {
            tracing::warn!("repo name: no remotefilelog.reponame");
            let path: String = input_config.get_or_default("paths", "default")?;
            name = repo_name_from_url(input_config, &path).unwrap_or_default();
            if name.is_empty() {
                tracing::warn!("repo name: no path.default reponame: {}", &path);
            }
            source = "paths.default";
        }
        if name.is_empty() {
            match read_repo_name_from_disk(repo_path) {
                Ok(s) => {
                    name = s;
                    source = "reponame file";
                }
                Err(e) => {
                    tracing::warn!("repo name: no reponame file: {:?}", &e);
                }
            };
        }
        (name, source)
    };

    if !repo_name.is_empty() {
        tracing::debug!("repo name: {:?} (from {})", &repo_name, source);
        if source != "reponame file" {
            let need_rewrite = match read_repo_name_from_disk(repo_path) {
                Ok(s) => s != repo_name,
                Err(_) => true,
            };
            if need_rewrite {
                let path = get_repo_name_path(repo_path);
                match fs::write(path, &repo_name) {
                    Ok(_) => tracing::debug!("repo name: written to reponame file"),
                    Err(e) => tracing::warn!("repo name: cannot write to reponame file: {:?}", e),
                }
            }
        }
        if source != "remotefilelog.reponame" {
            output_config.set(
                "remotefilelog",
                "reponame",
                Some(&repo_name),
                &Options::default().source(source).pin(false),
            );
        }
    } else {
        let forbid_empty_reponame: bool =
            input_config.get_or_default("configs", "forbid-empty-reponame")?;
        if forbid_empty_reponame && input_config.get("paths", "default").is_some() {
            let msg = "reponame is empty".to_string();
            return Err(Error::General(msg));
        }
    }

    Ok(repo_name)
}

trait ConfigSetExtInternal {
    fn load_user_internal(&mut self, path: Option<&PathBuf>, opts: Options) -> Vec<Error>;
}

impl ConfigSetExtInternal for ConfigSet {
    // For easier testing.
    fn load_user_internal(&mut self, path: Option<&PathBuf>, opts: Options) -> Vec<Error> {
        let mut errors = Vec::new();

        // Covert "$VISUAL", "$EDITOR" to "ui.editor".
        //
        // Unlike Mercurial, don't convert the "$PAGER" environment variable
        // to "pager.pager" config.
        //
        // The environment variable could be from the system profile (ex.
        // /etc/profile.d/...), or the user shell rc (ex. ~/.bashrc). There is
        // no clean way to tell which one it is from.  The value might be
        // tweaked for sysadmin usecases (ex. -n), which are different from
        // SCM's usecases.
        for name in ["VISUAL", "EDITOR"] {
            if let Ok(editor) = env::var(name) {
                if !editor.is_empty() {
                    self.set(
                        "ui",
                        "editor",
                        Some(editor),
                        &opts.clone().source(format!("${}", name)),
                    );
                    break;
                }
            }
        }

        // Convert $HGPROF to profiling.type
        if let Ok(profiling_type) = env::var("HGPROF") {
            self.set("profiling", "type", Some(profiling_type), &"$HGPROF".into());
        }

        let opts = opts.source("user").process_hgplain();

        if let Some(path) = path {
            errors.append(&mut self.load_path(path, &opts));
        }

        // Override ui.merge:interactive (source != user) with ui.merge
        // (source == user). This makes ui.merge in user hgrc effective,
        // even if ui.merge:interactive is not set.
        if self
            .get_sources("ui", "merge:interactive")
            .last()
            .map(|s| s.source().as_ref())
            != Some("user")
            && self
                .get_sources("ui", "merge")
                .last()
                .map(|s| s.source().as_ref())
                == Some("user")
        {
            if let Some(merge) = self.get("ui", "merge") {
                self.set("ui", "merge:interactive", Some(merge), &opts);
            }
        }

        errors
    }
}

/// Using custom "schemes" from config, resolve given url.
pub fn resolve_custom_scheme(config: &dyn Config, url: Url) -> Result<Url> {
    if let Some(tmpl) = config.get_nonempty("schemes", url.scheme()) {
        let non_scheme = match url.as_str().split_once(':') {
            Some((_, after)) => after.trim_start_matches('/'),
            None => bail!("url {url} has no scheme"),
        };

        let resolved_url = if tmpl.contains("{1}") {
            tmpl.replace("{1}", non_scheme)
        } else {
            format!("{tmpl}{non_scheme}")
        };

        return Url::parse(&resolved_url)
            .with_context(|| format!("parsing resolved custom scheme URL {resolved_url}"));
    }

    Ok(url)
}

pub fn repo_name_from_url(config: &dyn Config, s: &str) -> Option<String> {
    // Use a base_url to support non-absolute urls.
    let base_url = Url::parse("file:///.").unwrap();
    let parse_opts = Url::options().base_url(Some(&base_url));
    match parse_opts.parse(s) {
        Ok(url) => {
            let url = resolve_custom_scheme(config, url).ok()?;

            tracing::trace!("parsed url {}: {:?}", s, url);
            match url.scheme() {
                "mononoke" => {
                    // In Mononoke URLs, the repo name is always the full path
                    // with slashes trimmed.
                    let path = url.path().trim_matches('/');
                    if !path.is_empty() {
                        return Some(path.to_string());
                    }
                }
                _ => {
                    // Try to remove special prefixes to guess the repo name from that
                    if let Some(repo_prefix) = config.get("remotefilelog", "reponame-path-prefixes")
                    {
                        if let Some((_, reponame)) =
                            url.path().split_once(repo_prefix.to_string().as_str())
                        {
                            if !reponame.is_empty() {
                                return Some(reponame.to_string());
                            }
                        }
                    }
                    // Try the last segment in url path.
                    if let Some(last_segment) = url
                        .path_segments()
                        .and_then(|s| s.rev().find(|s| !s.is_empty()))
                    {
                        return Some(last_segment.to_string());
                    }
                    // Try path. `path_segment` can be `None` for URL like "test:reponame".
                    let path = url.path().trim_matches('/');
                    if !path.is_empty() {
                        return Some(path.to_string());
                    }
                    // Try the hostname. ex. in "fb://fbsource", "fbsource" is a host not a path.
                    // Also see https://www.mercurial-scm.org/repo/hg/help/schemes
                    if let Some(host_str) = url.host_str() {
                        return Some(host_str.to_string());
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!("cannot parse url {}: {:?}", s, e);
        }
    }
    None
}

#[cfg(feature = "fb")]
fn get_config_dir(info: Option<&RepoMinimalInfo>) -> Result<PathBuf, Error> {
    Ok(match info {
        Some(info) => info.shared_dot_hg_path.clone(),
        None => {
            let dirs = vec![
                std::env::var("TESTTMP")
                    .ok()
                    .map(|d| PathBuf::from(d).join(".cache")),
                std::env::var("HG_CONFIG_CACHE_DIR").ok().map(PathBuf::from),
                dirs::cache_dir(),
                Some(std::env::temp_dir()),
            ];

            let mut errs = vec![];
            for mut dir in dirs.into_iter().flatten() {
                dir.push("edenscm");
                match util::path::create_shared_dir_all(&dir) {
                    Err(err) => {
                        tracing::debug!("error setting up config cache dir {:?}: {}", dir, err);
                        errs.push((dir, err));
                        continue;
                    }
                    Ok(()) => return Ok(dir),
                }
            }

            return Err(Error::General(format!(
                "couldn't find config cache dir: {:?}",
                errs
            )));
        }
    })
}

#[cfg(feature = "fb")]
pub fn calculate_internalconfig(
    mode: FbConfigMode,
    config_dir: PathBuf,
    repo_name: Option<impl AsRef<str>>,
    canary: Option<String>,
    user_name: String,
    proxy_sock_path: Option<String>,
    allow_remote_snapshot: bool,
) -> Result<ConfigSet> {
    use crate::fb::internalconfig::Generator;
    Generator::new(
        mode,
        repo_name,
        config_dir,
        user_name,
        proxy_sock_path,
        allow_remote_snapshot,
    )?
    .execute(canary)
}

#[cfg(feature = "fb")]
pub fn generate_internalconfig(
    mode: FbConfigMode,
    info: Option<&RepoMinimalInfo>,
    repo_name: Option<impl AsRef<str>>,
    canary: Option<String>,
    user_name: String,
    proxy_sock_path: Option<String>,
    allow_remote_snapshot: bool,
) -> Result<()> {
    use std::io::Write;

    use filetime::set_file_mtime;
    use filetime::FileTime;
    use tempfile::tempfile_in;

    tracing::debug!(
        repo_path = ?info.map(|i| &i.path),
        canary = ?canary,
        "generate_internalconfig",
    );

    // Resolve sharedpath
    let config_dir = get_config_dir(info)?;

    // Verify that the filesystem is writable, otherwise exit early since we won't be able to write
    // the config.
    if tempfile_in(&config_dir).is_err() {
        return Err(IOError::new(
            ErrorKind::PermissionDenied,
            format!("no write access to {:?}", config_dir),
        )
        .into());
    }

    let version = ::version::VERSION;
    let header = format!(
        concat!(
            "# version={}\n",
            "# reponame={}\n",
            "# canary={:?}\n",
            "# username={}\n",
            "# Generated by `hg debugrefreshconfig` - DO NOT MODIFY\n",
        ),
        version,
        repo_name.as_ref().map_or("no_repo", |r| r.as_ref()),
        canary.as_ref(),
        &user_name,
    );

    let hgrc_path = config_dir.join("hgrc.dynamic");
    let global_config_dir = get_config_dir(None)?;

    let config = calculate_internalconfig(
        mode,
        global_config_dir,
        repo_name,
        canary,
        user_name,
        proxy_sock_path,
        allow_remote_snapshot,
    )?;
    let config_str = format!("{}{}", header, config);

    // If the file exists and will be unchanged, just update the mtime.
    if hgrc_path.exists() && read_to_string(&hgrc_path).unwrap_or_default() == config_str {
        let time = FileTime::now();
        tracing::debug!("bump {:?} mtime to {:?}", &hgrc_path, &time);
        set_file_mtime(hgrc_path, time)?;
    } else {
        tracing::debug!("rewrite {:?}", &hgrc_path);
        util::file::atomic_write(&hgrc_path, |f| {
            f.write_all(config_str.as_bytes())?;
            Ok(())
        })?;
    }

    Ok(())
}

/// Load the dynamic config files for the given repo path.
/// Returns errors parsing, generating, or fetching the configs.
#[cfg(feature = "fb")]
fn load_dynamic(
    info: Option<&RepoMinimalInfo>,
    opts: Options,
    identity: &Identity,
    proxy_sock_path: Option<String>,
    errors: &mut Vec<Error>,
) -> Result<ConfigSet> {
    use crate::fb::internalconfig::vpnless_config_path;

    let mode = FbConfigMode::from_identity(identity);
    let mut this = ConfigSet::new().named("dynamic");

    tracing::debug!("FbConfigMode is {:?}", &mode);

    if !mode.need_dynamic_generator() {
        return Ok(this);
    }

    // Compute path
    let dynamic_path = get_config_dir(info)?.join("hgrc.dynamic");

    // Check version
    let content = read_to_string(&dynamic_path).ok();
    let version = content.as_ref().and_then(|c| {
        let mut lines = c.split('\n');
        match lines.next() {
            Some(line) if line.starts_with("# version=") => Some(&line[10..]),
            Some(_) | None => None,
        }
    });

    let this_version = ::version::VERSION;

    let vpnless_changed = match (dynamic_path.metadata(), vpnless_config_path().metadata()) {
        (Ok(d), Ok(v)) => v.modified()? > d.modified()?,
        _ => false,
    };

    let needs_sync_generation =
            // No current dynamic config - need to generate.
            version.is_none()
            // VPNLess changed - need to regenerate.
            || vpnless_changed
            // Version mismatch between us and already generated - optionally generate.
            || !opts.minimize_dynamic_gen && version != Some(this_version);

    if needs_sync_generation {
        tracing::info!(?dynamic_path, file_version=?version, my_version=%this_version, vpnless_changed, "regenerating dynamic config (version mismatch)");
        let (repo_name, user_name) = {
            let mut temp_config = ConfigSet::new().named("temp");
            if !temp_config.load_user(opts.clone(), identity).is_empty() {
                bail!("unable to read user config to get user name");
            }

            let repo_name = match info {
                Some(info) => {
                    let opts = opts.clone().source("temp").process_hgplain();
                    // We need to know the repo name, but that's stored in the repository configs at
                    // the moment. In the long term we need to move that, but for now let's load the
                    // repo config ahead of time to read the name.
                    let repo_hgrc_path = info.dot_hg_path.join("hgrc");
                    if !temp_config.load_path(repo_hgrc_path, &opts).is_empty() {
                        bail!("unable to read repo config to get repo name");
                    }
                    Some(read_set_repo_name(
                        &temp_config,
                        &mut ConfigSet::new(),
                        &info.dot_hg_path,
                    )?)
                }
                None => None,
            };

            (repo_name, temp_config.get_or_default("ui", "username")?)
        };

        // Regen inline
        let res = generate_internalconfig(
            mode,
            info,
            repo_name,
            None,
            user_name,
            proxy_sock_path,
            // Allow using baked in remote config snapshot in case remote fetch fails.
            true,
        );
        if let Err(e) = res {
            let is_perm_error = e
                .chain()
                .any(|cause| match cause.downcast_ref::<IOError>() {
                    Some(io_error) if io_error.kind() == ErrorKind::PermissionDenied => true,
                    _ => false,
                });
            if !is_perm_error {
                return Err(e);
            }
        }
    } else {
        tracing::debug!(?dynamic_path, version=%this_version, "internalconfig version in-sync");
    }

    if !dynamic_path.exists() {
        return Err(IOError::new(
            ErrorKind::NotFound,
            format!("required config not found at {}", dynamic_path.display()),
        )
        .into());
    }

    // Read hgrc.dynamic
    let opts = opts.source("dynamic").process_hgplain();
    errors.append(&mut this.load_path(&dynamic_path, &opts));

    // Log config ages
    // - Done in python for now

    Ok(this)
}

/// Get the path of the reponame file.
fn get_repo_name_path(shared_dot_hg_path: &Path) -> PathBuf {
    shared_dot_hg_path.join("reponame")
}

/// Read repo name from shared `.hg` path.
pub fn read_repo_name_from_disk(shared_dot_hg_path: &Path) -> io::Result<String> {
    let repo_name_path = get_repo_name_path(shared_dot_hg_path);
    let name = fs::read_to_string(&repo_name_path)?.trim().to_string();
    if name.is_empty() {
        Err(IOError::new(
            ErrorKind::InvalidData,
            format!("reponame could not be empty ({})", repo_name_path.display()),
        ))
    } else {
        Ok(name)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::Write;

    use once_cell::sync::Lazy;
    use tempfile::TempDir;
    use testutil::envs::lock_env;

    use super::*;

    static CONFIG_ENV_VAR: Lazy<&str> =
        Lazy::new(|| identity::default().env_name_static("CONFIG").unwrap());
    static HGPLAIN: Lazy<&str> =
        Lazy::new(|| identity::default().env_name_static("PLAIN").unwrap());
    static HGPLAINEXCEPT: Lazy<&str> =
        Lazy::new(|| identity::default().env_name_static("PLAINEXCEPT").unwrap());

    fn write_file(path: PathBuf, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn test_basic_hgplain() {
        let mut env = lock_env();

        env.set(*HGPLAIN, Some("1"));
        env.set(*HGPLAINEXCEPT, None);

        let opts = Options::new().process_hgplain();
        let mut cfg = ConfigSet::new();
        cfg.parse(
            "[defaults]\n\
             commit = commit -d 0\n\
             [ui]\n\
             verbose = true\n\
             username = test\n\
             [alias]\n\
             l = log\n",
            &opts,
        );

        assert!(cfg.keys("defaults").is_empty());
        assert_eq!(cfg.get("ui", "verbose"), None);
        assert_eq!(cfg.get("ui", "username"), Some("test".into()));
        assert_eq!(cfg.get("alias", "l"), None);
    }

    #[test]
    fn test_static_config_hgplain() {
        let mut env = lock_env();

        for id in identity::all() {
            env.set(id.env_name_static("PLAIN").unwrap(), None);
            env.set(id.env_name_static("PLAINEXCEPT").unwrap(), None);
        }

        env.set("TESTTMP", Some("1"));

        let cfg = load(None, &[]).unwrap();

        // Sanity that we have a test value from static config.
        assert_eq!(
            cfg.get("alias", "some-command"),
            Some("some-command --some-flag".into())
        );
        let sources = cfg.get_sources("alias", "some-command");
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source(), &"builtin:test_config");

        // With HGPLAIN=1, aliases should get dropped.
        env.set(*HGPLAIN, Some("1"));
        let cfg = load(None, &[]).unwrap();
        assert_eq!(cfg.get("alias", "some-command"), None);
    }

    #[test]
    fn test_hgplainexcept() {
        let mut env = lock_env();

        env.set(*HGPLAIN, None);
        env.set(*HGPLAINEXCEPT, Some("alias,revsetalias"));

        let opts = Options::new().process_hgplain();
        let mut cfg = ConfigSet::new();
        cfg.parse(
            "[defaults]\n\
             commit = commit -d 0\n\
             [alias]\n\
             l = log\n\
             [templatealias]\n\
             u = user\n\
             [revsetalias]\n\
             @ = master\n",
            &opts,
        );

        assert!(cfg.keys("defaults").is_empty());
        assert_eq!(cfg.get("alias", "l"), Some("log".into()));
        assert_eq!(cfg.get("revsetalias", "@"), Some("master".into()));
        assert_eq!(cfg.get("templatealias", "u"), None);
    }

    #[test]
    fn test_is_plain() {
        let mut env = lock_env();

        use hgplain::is_plain;

        for id in identity::all() {
            env.set(id.env_name_static("PLAIN").unwrap(), None);
            env.set(id.env_name_static("PLAINEXCEPT").unwrap(), None);
        }

        assert!(!is_plain(None));

        env.set(*HGPLAIN, Some("1"));
        assert!(is_plain(None));
        assert!(is_plain(Some("banana")));

        env.set(*HGPLAINEXCEPT, Some("dog,banana,tree"));
        assert!(!is_plain(Some("banana")));

        env.set(*HGPLAIN, None);
        assert!(!is_plain(Some("banana")));
    }

    #[test]
    fn test_config_path() {
        let mut env = lock_env();

        let dir = TempDir::with_prefix("test_config_path.").unwrap();

        write_file(dir.path().join("1.rc"), "[x]\na=1");
        write_file(dir.path().join("2.rc"), "[y]\nb=2");
        write_file(dir.path().join("user.rc"), "");

        let hgrcpath = &[
            dir.path().join("1.rc").display().to_string(),
            dir.path().join("2.rc").display().to_string(),
            format!("user={}", dir.path().join("user.rc").display()),
        ]
        .join(";");
        env.set(*CONFIG_ENV_VAR, Some(hgrcpath));

        let mut cfg = ConfigSet::new();

        let identity = identity::default();
        cfg.load_user(Options::new(), &identity);
        assert_eq!(cfg.get("x", "a"), None);

        let identity = identity::default();
        cfg.load_system(Options::new(), &identity);
        assert_eq!(cfg.get("x", "a"), Some("1".into()));
        assert_eq!(cfg.get("y", "b"), Some("2".into()));
    }

    #[test]
    fn test_load_user() {
        let _env = lock_env();

        let dir = TempDir::with_prefix("test_hgrcpath.").unwrap();
        let path = dir.path().join("1.rc");

        write_file(path.clone(), "[ui]\nmerge=x");

        let mut cfg = ConfigSet::new();
        cfg.load_user_internal(Some(&path), Options::new());
        assert_eq!(cfg.get("ui", "merge").unwrap(), "x");
        assert_eq!(cfg.get("ui", "merge:interactive").unwrap(), "x");

        let mut cfg = ConfigSet::new();
        cfg.set("ui", "merge", Some("foo"), &"system".into());
        cfg.set("ui", "merge:interactive", Some("foo"), &"system".into());
        cfg.load_user_internal(Some(&path), Options::new());
        assert_eq!(cfg.get("ui", "merge").unwrap(), "x");
        assert_eq!(cfg.get("ui", "merge:interactive").unwrap(), "x");

        let mut cfg = ConfigSet::new();
        cfg.set("ui", "merge:interactive", Some("foo"), &"system".into());
        write_file(path.clone(), "[ui]\nmerge=x\nmerge:interactive=y\n");
        cfg.load_user_internal(Some(&path), Options::new());
        assert_eq!(cfg.get("ui", "merge").unwrap(), "x");
        assert_eq!(cfg.get("ui", "merge:interactive").unwrap(), "y");

        let mut cfg = ConfigSet::new();
        cfg.set("ui", "merge", Some("a"), &"system".into());
        cfg.set("ui", "merge:interactive", Some("b"), &"system".into());
        write_file(path.clone(), "");
        cfg.load_user_internal(Some(&path), Options::new());
        assert_eq!(cfg.get("ui", "merge").unwrap(), "a");
        assert_eq!(cfg.get("ui", "merge:interactive").unwrap(), "b");
        write_file(path.clone(), "[ui]\nmerge:interactive=y\n");
        cfg.load_user_internal(Some(&path), Options::new());
        assert_eq!(cfg.get("ui", "merge").unwrap(), "a");
        assert_eq!(cfg.get("ui", "merge:interactive").unwrap(), "y");

        drop(path);
    }

    #[test]
    fn test_load_hgrc() {
        let dir = TempDir::with_prefix("test_hgrcpath.").unwrap();
        let path = dir.path().join("1.rc");

        write_file(path.clone(), "[x]\na=1\n[alias]\nb=c\n");

        let mut env = lock_env();

        for id in identity::all() {
            env.set(id.env_name_static("PLAIN").unwrap(), None);
            env.set(id.env_name_static("PLAINEXCEPT").unwrap(), None);
        }

        env.set(*HGPLAIN, Some("1"));
        env.set(*HGPLAINEXCEPT, None);

        let mut cfg = ConfigSet::new();
        cfg.load_hgrc(&path, "hgrc");

        assert!(cfg.keys("alias").is_empty());
        assert!(cfg.get("alias", "b").is_none());
        assert_eq!(cfg.get("x", "a").unwrap(), "1");

        env.set(*HGPLAIN, None);
        cfg.load_hgrc(&path, "hgrc");

        assert_eq!(cfg.get("alias", "b").unwrap(), "c");
    }

    #[test]
    fn test_section_filter() {
        let opts = Options::new().filter_sections(vec!["x", "y"]);
        let mut cfg = ConfigSet::new();
        cfg.parse(
            "[x]\n\
             a=1\n\
             [y]\n\
             b=2\n\
             [z]\n\
             c=3",
            &opts,
        );

        assert_eq!(cfg.sections(), vec![Text::from("x"), Text::from("y")]);
        assert_eq!(cfg.get("z", "c"), None);
    }

    #[test]
    fn test_section_remap() {
        let mut remap = HashMap::new();
        remap.insert("x", "y");
        remap.insert("y", "z");

        let opts = Options::new().remap_sections(remap);
        let mut cfg = ConfigSet::new();
        cfg.parse(
            "[x]\n\
             a=1\n\
             [y]\n\
             b=2\n\
             [z]\n\
             c=3",
            &opts,
        );

        assert_eq!(cfg.get("y", "a"), Some("1".into()));
        assert_eq!(cfg.get("z", "b"), Some("2".into()));
        assert_eq!(cfg.get("z", "c"), Some("3".into()));
    }

    #[test]
    fn test_py_core_items() {
        let mut env = lock_env();

        // Skip real dynamic config.
        env.set("TESTTMP", Some("1"));

        let mut cfg = ConfigSet::new();
        cfg.load(None, Default::default()).unwrap();
        assert_eq!(cfg.get("treestate", "repackfactor").unwrap(), "3");
    }

    #[test]
    fn test_load_cli_args() {
        let mut env = lock_env();

        // Skip real dynamic config.
        env.set("TESTTMP", Some("1"));

        let dir = TempDir::with_prefix("test_load.").unwrap();

        let repo_rc = dir.path().join(".sl/config");
        write_file(repo_rc, "[s]\na=orig\nb=orig\nc=orig");

        let other_rc = dir.path().join("other.rc");
        write_file(other_rc.clone(), "[s]\na=other\nb=other");

        write_file(dir.path().join(".sl/requires"), "treestate\n");

        let repo = RepoMinimalInfo::from_repo_root(dir.path().to_path_buf()).unwrap();

        let cfg = load(
            Some(&repo),
            &[
                PinnedConfig::File(
                    format!("{}", other_rc.display()).into(),
                    "--configfile".into(),
                ),
                PinnedConfig::Raw("s.b=flag".into(), "--config".into()),
            ],
        )
        .unwrap();

        assert_eq!(cfg.get("s", "a"), Some("other".into()));
        assert_eq!(cfg.get("s", "b"), Some("flag".into()));
        assert_eq!(cfg.get("s", "c"), Some("orig".into()));
    }

    #[test]
    fn test_repo_name_from_url() {
        let config = BTreeMap::<&str, &str>::from([("schemes.fb", "mononoke://example.com/{1}")]);

        let check = |url, name| {
            assert_eq!(repo_name_from_url(&config, url).as_deref(), name);
        };

        // Ordinary schemes use the basename as the repo name
        check("repo", Some("repo"));
        check("../path/to/repo", Some("repo"));
        check("file:repo", Some("repo"));
        check("file:/path/to/repo", Some("repo"));
        check("file://server/path/to/repo", Some("repo"));
        check("ssh://user@host/repo", Some("repo"));
        check("ssh://user@host/path/to/repo", Some("repo"));
        check("file:/", None);

        // This isn't correct, but is a side-effect of earlier hacks (should
        // be `None`)
        check("ssh://user@host:100/", Some("host"));

        // Mononoke scheme uses the full path, and repo names can contain
        // slashes.
        check("mononoke://example.com/repo", Some("repo"));
        check("mononoke://example.com/path/to/repo", Some("path/to/repo"));
        check("mononoke://example.com/", None);

        // FB scheme uses the full path.
        check("fb:repo", Some("repo"));
        check("fb:path/to/repo", Some("path/to/repo"));
        check("fb:", None);

        // FB scheme works even when there are extra slashes that shouldn't be
        // there.
        check("fb://repo/", Some("repo"));
        check("fb://path/to/repo", Some("path/to/repo"));
    }

    #[test]
    fn test_resolve_custom_scheme() {
        let config = BTreeMap::<&str, &str>::from([
            ("schemes.append", "appended://bar/"),
            ("schemes.subst", "substd://bar/{1}/baz"),
        ]);

        let check = |url, resolved| {
            assert_eq!(
                resolve_custom_scheme(&config, Url::parse(url).unwrap())
                    .unwrap()
                    .as_str(),
                resolved
            );
        };

        check("other://foo", "other://foo");
        check("append:one/two", "appended://bar/one/two");
        check("subst://one/two", "substd://bar/one/two/baz");
    }
}
