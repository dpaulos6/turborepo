mod env;
mod file;
mod turbo_json;

use std::{collections::HashMap, ffi::OsString, io};

use camino::{Utf8Path, Utf8PathBuf};
use convert_case::{Case, Casing};
use env::{get_env_var_config, get_override_env_var_config};
use file::{get_global_auth, get_global_config, get_local_config};
use miette::{Diagnostic, NamedSource, SourceSpan};
use serde::Deserialize;
use struct_iterable::Iterable;
use thiserror::Error;
use turbopath::AbsoluteSystemPathBuf;
use turborepo_errors::TURBO_SITE;

pub use crate::turbo_json::{RawTurboJson, UIMode};
use crate::{cli::EnvMode, commands::CommandBase};

#[derive(Debug, Error, Diagnostic)]
#[error("Environment variables should not be prefixed with \"{env_pipeline_delimiter}\"")]
#[diagnostic(
    code(invalid_env_prefix),
    url("{}/messages/{}", TURBO_SITE, self.code().unwrap().to_string().to_case(Case::Kebab))
)]
pub struct InvalidEnvPrefixError {
    pub value: String,
    pub key: String,
    #[source_code]
    pub text: NamedSource,
    #[label("variable with invalid prefix declared here")]
    pub span: Option<SourceSpan>,
    pub env_pipeline_delimiter: &'static str,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error, Diagnostic)]
pub enum Error {
    #[error("Authentication error: {0}")]
    Auth(#[from] turborepo_auth::Error),
    #[error("Global config path not found")]
    NoGlobalConfigPath,
    #[error("Global auth file path not found")]
    NoGlobalAuthFilePath,
    #[error("Global config directory not found")]
    NoGlobalConfigDir,
    #[error(transparent)]
    PackageJson(#[from] turborepo_repository::package_json::Error),
    #[error(
        "Could not find turbo.json.\nFollow directions at https://turbo.build/repo/docs to create \
         one"
    )]
    NoTurboJSON,
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Camino(#[from] camino::FromPathBufError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("Encountered an IO error while attempting to read {config_path}: {error}")]
    FailedToReadConfig {
        config_path: AbsoluteSystemPathBuf,
        error: io::Error,
    },
    #[error("Encountered an IO error while attempting to set {config_path}: {error}")]
    FailedToSetConfig {
        config_path: AbsoluteSystemPathBuf,
        error: io::Error,
    },
    #[error(
        "Package tasks (<package>#<task>) are not allowed in single-package repositories: found \
         {task_id}"
    )]
    #[diagnostic(code(package_task_in_single_package_mode), url("{}/messages/{}", TURBO_SITE, self.code().unwrap().to_string().to_case(Case::Kebab)))]
    PackageTaskInSinglePackageMode {
        task_id: String,
        #[source_code]
        text: NamedSource,
        #[label("package task found here")]
        span: Option<SourceSpan>,
    },
    #[error(transparent)]
    #[diagnostic(transparent)]
    InvalidEnvPrefix(Box<InvalidEnvPrefixError>),
    #[error(transparent)]
    PathError(#[from] turbopath::PathError),
    #[diagnostic(
        code(unnecessary_package_task_syntax),
        url("{}/messages/{}", TURBO_SITE, self.code().unwrap().to_string().to_case(Case::Kebab))
    )]
    #[error("\"{actual}\". Use \"{wanted}\" instead")]
    UnnecessaryPackageTaskSyntax {
        actual: String,
        wanted: String,
        #[label("unnecessary package syntax found here")]
        span: Option<SourceSpan>,
        #[source_code]
        text: NamedSource,
    },
    #[error("You can only extend from the root workspace")]
    ExtendFromNonRoot {
        #[label("non-root workspace found here")]
        span: Option<SourceSpan>,
        #[source_code]
        text: NamedSource,
    },
    #[error("`{field}` cannot contain an environment variable")]
    InvalidDependsOnValue {
        field: &'static str,
        #[label("environment variable found here")]
        span: Option<SourceSpan>,
        #[source_code]
        text: NamedSource,
    },
    #[error("`{field}` cannot contain an absolute path")]
    AbsolutePathInConfig {
        field: &'static str,
        #[label("absolute path found here")]
        span: Option<SourceSpan>,
        #[source_code]
        text: NamedSource,
    },
    #[error("No \"extends\" key found")]
    NoExtends {
        #[label("add extends key here")]
        span: Option<SourceSpan>,
        #[source_code]
        text: NamedSource,
    },
    #[error("Tasks cannot be marked as interactive and cacheable")]
    InteractiveNoCacheable {
        #[label("marked interactive here")]
        span: Option<SourceSpan>,
        #[source_code]
        text: NamedSource,
    },
    #[error("found `pipeline` field instead of `tasks`")]
    #[diagnostic(help("changed in 2.0: `pipeline` has been renamed to `tasks`"))]
    PipelineField {
        #[label("rename `pipeline` field to `tasks`")]
        span: Option<SourceSpan>,
        #[source_code]
        text: NamedSource,
    },
    #[error("Failed to create APIClient: {0}")]
    ApiClient(#[source] turborepo_api_client::Error),
    #[error("{0} is not UTF8.")]
    Encoding(String),
    #[error("TURBO_SIGNATURE should be either 1 or 0.")]
    InvalidSignature,
    #[error("TURBO_REMOTE_CACHE_ENABLED should be either 1 or 0.")]
    InvalidRemoteCacheEnabled,
    #[error("TURBO_REMOTE_CACHE_TIMEOUT: error parsing timeout.")]
    InvalidRemoteCacheTimeout(#[source] std::num::ParseIntError),
    #[error("TURBO_REMOTE_CACHE_UPLOAD_TIMEOUT: error parsing timeout.")]
    InvalidUploadTimeout(#[source] std::num::ParseIntError),
    #[error("TURBO_PREFLIGHT should be either 1 or 0.")]
    InvalidPreflight,
    #[error(transparent)]
    #[diagnostic(transparent)]
    TurboJsonParseError(#[from] crate::turbo_json::parser::Error),
    #[error("found absolute path in `cacheDir`")]
    #[diagnostic(help("if absolute paths are required, use `--cache-dir` or `TURBO_CACHE_DIR`"))]
    AbsoluteCacheDir {
        #[label("make `cacheDir` value a relative unix path")]
        span: Option<SourceSpan>,
        #[source_code]
        text: NamedSource,
    },
}

macro_rules! create_builder {
    ($func_name:ident, $property_name:ident, $type:ty) => {
        pub fn $func_name(mut self, value: $type) -> Self {
            self.override_config.$property_name = value;
            self
        }
    };
}

const DEFAULT_API_URL: &str = "https://vercel.com/api";
const DEFAULT_LOGIN_URL: &str = "https://vercel.com";
const DEFAULT_TIMEOUT: u64 = 30;
const DEFAULT_UPLOAD_TIMEOUT: u64 = 60;

// We intentionally don't derive Serialize so that different parts
// of the code that want to display the config can tune how they
// want to display and what fields they want to include.
#[derive(Deserialize, Default, Debug, PartialEq, Eq, Clone, Iterable)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationOptions {
    #[serde(alias = "apiurl")]
    #[serde(alias = "ApiUrl")]
    #[serde(alias = "APIURL")]
    pub(crate) api_url: Option<String>,
    #[serde(alias = "loginurl")]
    #[serde(alias = "LoginUrl")]
    #[serde(alias = "LOGINURL")]
    pub(crate) login_url: Option<String>,
    #[serde(alias = "teamslug")]
    #[serde(alias = "TeamSlug")]
    #[serde(alias = "TEAMSLUG")]
    pub(crate) team_slug: Option<String>,
    #[serde(alias = "teamid")]
    #[serde(alias = "TeamId")]
    #[serde(alias = "TEAMID")]
    pub(crate) team_id: Option<String>,
    pub(crate) token: Option<String>,
    pub(crate) signature: Option<bool>,
    pub(crate) preflight: Option<bool>,
    pub(crate) timeout: Option<u64>,
    pub(crate) upload_timeout: Option<u64>,
    pub(crate) enabled: Option<bool>,
    pub(crate) spaces_id: Option<String>,
    #[serde(rename = "ui")]
    pub(crate) ui: Option<UIMode>,
    #[serde(rename = "dangerouslyDisablePackageManagerCheck")]
    pub(crate) allow_no_package_manager: Option<bool>,
    pub(crate) daemon: Option<bool>,
    #[serde(rename = "envMode")]
    pub(crate) env_mode: Option<EnvMode>,
    pub(crate) scm_base: Option<String>,
    pub(crate) scm_head: Option<String>,
    #[serde(rename = "cacheDir")]
    pub(crate) cache_dir: Option<Utf8PathBuf>,
}

#[derive(Default)]
pub struct TurborepoConfigBuilder {
    repo_root: AbsoluteSystemPathBuf,
    override_config: ConfigurationOptions,
    global_config_path: Option<AbsoluteSystemPathBuf>,
    #[cfg(test)]
    environment: HashMap<OsString, OsString>,
}

// Getters
impl ConfigurationOptions {
    pub fn api_url(&self) -> &str {
        non_empty_str(self.api_url.as_deref()).unwrap_or(DEFAULT_API_URL)
    }

    pub fn login_url(&self) -> &str {
        non_empty_str(self.login_url.as_deref()).unwrap_or(DEFAULT_LOGIN_URL)
    }

    pub fn team_slug(&self) -> Option<&str> {
        self.team_slug
            .as_deref()
            .and_then(|slug| (!slug.is_empty()).then_some(slug))
    }

    pub fn team_id(&self) -> Option<&str> {
        non_empty_str(self.team_id.as_deref())
    }

    pub fn token(&self) -> Option<&str> {
        non_empty_str(self.token.as_deref())
    }

    pub fn signature(&self) -> bool {
        self.signature.unwrap_or_default()
    }

    pub fn enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    pub fn preflight(&self) -> bool {
        self.preflight.unwrap_or_default()
    }

    /// Note: 0 implies no timeout
    pub fn timeout(&self) -> u64 {
        self.timeout.unwrap_or(DEFAULT_TIMEOUT)
    }

    /// Note: 0 implies no timeout
    pub fn upload_timeout(&self) -> u64 {
        self.upload_timeout.unwrap_or(DEFAULT_UPLOAD_TIMEOUT)
    }

    pub fn spaces_id(&self) -> Option<&str> {
        self.spaces_id.as_deref()
    }

    pub fn ui(&self) -> UIMode {
        // If we aren't hooked up to a TTY, then do not use TUI
        if !atty::is(atty::Stream::Stdout) {
            return UIMode::Stream;
        }

        self.ui.unwrap_or(UIMode::Stream)
    }

    pub fn scm_base(&self) -> Option<&str> {
        non_empty_str(self.scm_base.as_deref())
    }

    pub fn scm_head(&self) -> &str {
        non_empty_str(self.scm_head.as_deref()).unwrap_or("HEAD")
    }

    pub fn allow_no_package_manager(&self) -> bool {
        self.allow_no_package_manager.unwrap_or_default()
    }

    pub fn daemon(&self) -> Option<bool> {
        self.daemon
    }

    pub fn env_mode(&self) -> EnvMode {
        self.env_mode.unwrap_or_default()
    }

    pub fn cache_dir(&self) -> &Utf8Path {
        self.cache_dir.as_deref().unwrap_or_else(|| {
            Utf8Path::new(if cfg!(windows) {
                ".turbo\\cache"
            } else {
                ".turbo/cache"
            })
        })
    }
}

macro_rules! create_set_if_empty {
    ($func_name:ident, $property_name:ident, $type:ty) => {
        fn $func_name(&mut self, value: &mut Option<$type>) {
            if self.$property_name.is_none() {
                if let Some(value) = value.take() {
                    self.$property_name = Some(value);
                }
            }
        }
    };
}

// Private setters used only for construction
impl ConfigurationOptions {
    create_set_if_empty!(set_api_url, api_url, String);
    create_set_if_empty!(set_login_url, login_url, String);
    create_set_if_empty!(set_team_slug, team_slug, String);
    create_set_if_empty!(set_team_id, team_id, String);
    create_set_if_empty!(set_token, token, String);
    create_set_if_empty!(set_signature, signature, bool);
    create_set_if_empty!(set_enabled, enabled, bool);
    create_set_if_empty!(set_preflight, preflight, bool);
    create_set_if_empty!(set_timeout, timeout, u64);
    create_set_if_empty!(set_ui, ui, UIMode);
    create_set_if_empty!(set_allow_no_package_manager, allow_no_package_manager, bool);
    create_set_if_empty!(set_daemon, daemon, bool);
    create_set_if_empty!(set_env_mode, env_mode, EnvMode);
    create_set_if_empty!(set_cache_dir, cache_dir, Utf8PathBuf);
    create_set_if_empty!(set_scm_base, scm_base, String);
    create_set_if_empty!(set_scm_head, scm_head, String);
    create_set_if_empty!(set_spaces_id, spaces_id, String);
}

// Maps Some("") to None to emulate how Go handles empty strings
fn non_empty_str(s: Option<&str>) -> Option<&str> {
    s.filter(|s| !s.is_empty())
}

trait ResolvedConfigurationOptions {
    fn get_configuration_options(self) -> Result<ConfigurationOptions, Error>;
}

// Used for global config and local config.
impl ResolvedConfigurationOptions for ConfigurationOptions {
    fn get_configuration_options(self) -> Result<ConfigurationOptions, Error> {
        Ok(self)
    }
}

fn get_lowercased_env_vars() -> HashMap<OsString, OsString> {
    std::env::vars_os()
        .map(|(k, v)| (k.to_ascii_lowercase(), v))
        .collect()
}

impl TurborepoConfigBuilder {
    pub fn new(base: &CommandBase) -> Self {
        Self {
            repo_root: base.repo_root.to_owned(),
            override_config: Default::default(),
            #[cfg(not(test))]
            global_config_path: None,
            #[cfg(test)]
            global_config_path: base.global_config_path.clone(),
            #[cfg(test)]
            environment: Default::default(),
        }
    }

    // Getting all of the paths.
    #[allow(dead_code)]
    fn root_package_json_path(&self) -> AbsoluteSystemPathBuf {
        self.repo_root.join_component("package.json")
    }
    #[allow(dead_code)]
    fn root_turbo_json_path(&self) -> AbsoluteSystemPathBuf {
        self.repo_root.join_component("turbo.json")
    }

    #[cfg(test)]
    fn get_environment(&self) -> HashMap<OsString, OsString> {
        self.environment.clone()
    }

    #[cfg(not(test))]
    fn get_environment(&self) -> HashMap<OsString, OsString> {
        get_lowercased_env_vars()
    }

    create_builder!(with_api_url, api_url, Option<String>);
    create_builder!(with_login_url, login_url, Option<String>);
    create_builder!(with_team_slug, team_slug, Option<String>);
    create_builder!(with_team_id, team_id, Option<String>);
    create_builder!(with_token, token, Option<String>);
    create_builder!(with_signature, signature, Option<bool>);
    create_builder!(with_enabled, enabled, Option<bool>);
    create_builder!(with_preflight, preflight, Option<bool>);
    create_builder!(with_timeout, timeout, Option<u64>);
    create_builder!(with_ui, ui, Option<UIMode>);
    create_builder!(
        with_allow_no_package_manager,
        allow_no_package_manager,
        Option<bool>
    );
    create_builder!(with_daemon, daemon, Option<bool>);
    create_builder!(with_env_mode, env_mode, Option<EnvMode>);
    create_builder!(with_cache_dir, cache_dir, Option<Utf8PathBuf>);

    pub fn build(&self) -> Result<ConfigurationOptions, Error> {
        // Priority, from least significant to most significant:
        // - shared configuration (turbo.json)
        // - global configuration (~/.turbo/config.json)
        // - local configuration (<REPO_ROOT>/.turbo/config.json)
        // - environment variables
        // - CLI arguments
        // - builder pattern overrides.

        let turbo_json = RawTurboJson::read(
            &self.repo_root,
            &self.repo_root.join_component("turbo.json"),
        )
        .or_else(|e| {
            if let Error::Io(e) = &e {
                if matches!(e.kind(), std::io::ErrorKind::NotFound) {
                    return Ok(Default::default());
                }
            }

            Err(e)
        })?;
        let global_config = get_global_config(self.global_config_path.clone())?;
        let global_auth = get_global_auth(self.global_config_path.clone())?;
        let local_config = get_local_config(&self.repo_root)?;
        let env_vars = self.get_environment();
        let env_var_config = get_env_var_config(&env_vars)?;
        let override_env_var_config = get_override_env_var_config(&env_vars)?;

        // These are ordered from highest to lowest priority
        let sources = [
            override_env_var_config.get_configuration_options(),
            Ok(self.override_config.clone()),
            env_var_config.get_configuration_options(),
            local_config.get_configuration_options(),
            global_auth.get_configuration_options(),
            global_config.get_configuration_options(),
            turbo_json.get_configuration_options(),
        ];

        sources.into_iter().try_fold(
            ConfigurationOptions::default(),
            |mut acc, current_source| {
                current_source.map(|mut current_source_config| {
                    acc.set_api_url(&mut current_source_config.api_url);
                    acc.set_login_url(&mut current_source_config.login_url);
                    acc.set_team_slug(&mut current_source_config.team_slug);
                    acc.set_team_id(&mut current_source_config.team_id);
                    acc.set_token(&mut current_source_config.token);
                    acc.set_signature(&mut current_source_config.signature);
                    acc.set_enabled(&mut current_source_config.enabled);
                    acc.set_preflight(&mut current_source_config.preflight);
                    acc.set_timeout(&mut current_source_config.timeout);
                    acc.set_spaces_id(&mut current_source_config.spaces_id);
                    acc.set_ui(&mut current_source_config.ui);
                    acc.set_allow_no_package_manager(
                        &mut current_source_config.allow_no_package_manager,
                    );
                    acc.set_daemon(&mut current_source_config.daemon);
                    acc.set_env_mode(&mut current_source_config.env_mode);
                    acc.set_scm_base(&mut current_source_config.scm_base);
                    acc.set_scm_head(&mut current_source_config.scm_head);
                    acc.set_cache_dir(&mut current_source_config.cache_dir);
                    acc
                })
            },
        )
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, ffi::OsString};

    use camino::Utf8PathBuf;
    use tempfile::TempDir;
    use turbopath::AbsoluteSystemPathBuf;

    use crate::{
        cli::EnvMode,
        config::{
            get_env_var_config, get_override_env_var_config, ConfigurationOptions,
            TurborepoConfigBuilder, DEFAULT_API_URL, DEFAULT_LOGIN_URL, DEFAULT_TIMEOUT,
        },
        turbo_json::UIMode,
    };

    #[test]
    fn test_defaults() {
        let defaults: ConfigurationOptions = Default::default();
        assert_eq!(defaults.api_url(), DEFAULT_API_URL);
        assert_eq!(defaults.login_url(), DEFAULT_LOGIN_URL);
        assert_eq!(defaults.team_slug(), None);
        assert_eq!(defaults.team_id(), None);
        assert_eq!(defaults.token(), None);
        assert!(!defaults.signature());
        assert!(defaults.enabled());
        assert!(!defaults.preflight());
        assert_eq!(defaults.timeout(), DEFAULT_TIMEOUT);
        assert_eq!(defaults.spaces_id(), None);
        assert!(!defaults.allow_no_package_manager());
    }

    #[test]
    fn test_env_setting() {
        let mut env: HashMap<OsString, OsString> = HashMap::new();

        let turbo_api = "https://example.com/api";
        let turbo_login = "https://example.com/login";
        let turbo_team = "vercel";
        let turbo_teamid = "team_nLlpyC6REAqxydlFKbrMDlud";
        let turbo_token = "abcdef1234567890abcdef";
        let cache_dir = Utf8PathBuf::from("nebulo9");
        let turbo_remote_cache_timeout = 200;

        env.insert("turbo_api".into(), turbo_api.into());
        env.insert("turbo_login".into(), turbo_login.into());
        env.insert("turbo_team".into(), turbo_team.into());
        env.insert("turbo_teamid".into(), turbo_teamid.into());
        env.insert("turbo_token".into(), turbo_token.into());
        env.insert(
            "turbo_remote_cache_timeout".into(),
            turbo_remote_cache_timeout.to_string().into(),
        );
        env.insert("turbo_ui".into(), "true".into());
        env.insert(
            "turbo_dangerously_disable_package_manager_check".into(),
            "true".into(),
        );
        env.insert("turbo_daemon".into(), "true".into());
        env.insert("turbo_preflight".into(), "true".into());
        env.insert("turbo_env_mode".into(), "strict".into());
        env.insert("turbo_cache_dir".into(), cache_dir.clone().into());

        let config = get_env_var_config(&env).unwrap();
        assert!(config.preflight());
        assert_eq!(turbo_api, config.api_url.unwrap());
        assert_eq!(turbo_login, config.login_url.unwrap());
        assert_eq!(turbo_team, config.team_slug.unwrap());
        assert_eq!(turbo_teamid, config.team_id.unwrap());
        assert_eq!(turbo_token, config.token.unwrap());
        assert_eq!(turbo_remote_cache_timeout, config.timeout.unwrap());
        assert_eq!(Some(UIMode::Tui), config.ui);
        assert_eq!(Some(true), config.allow_no_package_manager);
        assert_eq!(Some(true), config.daemon);
        assert_eq!(Some(EnvMode::Strict), config.env_mode);
        assert_eq!(cache_dir, config.cache_dir.unwrap());
    }

    #[test]
    fn test_empty_env_setting() {
        let mut env: HashMap<OsString, OsString> = HashMap::new();
        env.insert("turbo_api".into(), "".into());
        env.insert("turbo_login".into(), "".into());
        env.insert("turbo_team".into(), "".into());
        env.insert("turbo_teamid".into(), "".into());
        env.insert("turbo_token".into(), "".into());
        env.insert("turbo_ui".into(), "".into());
        env.insert("turbo_daemon".into(), "".into());
        env.insert("turbo_env_mode".into(), "".into());
        env.insert("turbo_preflight".into(), "".into());
        env.insert("turbo_scm_head".into(), "".into());
        env.insert("turbo_scm_base".into(), "".into());

        let config = get_env_var_config(&env).unwrap();
        assert_eq!(config.api_url(), DEFAULT_API_URL);
        assert_eq!(config.login_url(), DEFAULT_LOGIN_URL);
        assert_eq!(config.team_slug(), None);
        assert_eq!(config.team_id(), None);
        assert_eq!(config.token(), None);
        assert_eq!(config.ui, None);
        assert_eq!(config.daemon, None);
        assert_eq!(config.env_mode, None);
        assert!(!config.preflight());
        assert_eq!(config.scm_base(), None);
        assert_eq!(config.scm_head(), "HEAD");
    }

    #[test]
    fn test_override_env_setting() {
        let mut env: HashMap<OsString, OsString> = HashMap::new();

        let vercel_artifacts_token = "correct-horse-battery-staple";
        let vercel_artifacts_owner = "bobby_tables";

        env.insert(
            "vercel_artifacts_token".into(),
            vercel_artifacts_token.into(),
        );
        env.insert(
            "vercel_artifacts_owner".into(),
            vercel_artifacts_owner.into(),
        );
        env.insert("ci".into(), "1".into());

        let config = get_override_env_var_config(&env).unwrap();
        assert_eq!(vercel_artifacts_token, config.token.unwrap());
        assert_eq!(vercel_artifacts_owner, config.team_id.unwrap());
        assert_eq!(Some(UIMode::Stream), config.ui);
    }

    #[test]
    fn test_env_layering() {
        let tmp_dir = TempDir::new().unwrap();
        let repo_root = AbsoluteSystemPathBuf::try_from(tmp_dir.path()).unwrap();
        let global_config_path = AbsoluteSystemPathBuf::try_from(
            TempDir::new().unwrap().path().join("nonexistent.json"),
        )
        .unwrap();

        repo_root
            .join_component("turbo.json")
            .create_with_contents(r#"{"experimentalSpaces": {"id": "my-spaces-id"}}"#)
            .unwrap();

        let turbo_teamid = "team_nLlpyC6REAqxydlFKbrMDlud";
        let turbo_token = "abcdef1234567890abcdef";
        let vercel_artifacts_owner = "team_SOMEHASH";
        let vercel_artifacts_token = "correct-horse-battery-staple";

        let mut env: HashMap<OsString, OsString> = HashMap::new();
        env.insert("turbo_teamid".into(), turbo_teamid.into());
        env.insert("turbo_token".into(), turbo_token.into());
        env.insert(
            "vercel_artifacts_token".into(),
            vercel_artifacts_token.into(),
        );
        env.insert(
            "vercel_artifacts_owner".into(),
            vercel_artifacts_owner.into(),
        );

        let override_config = ConfigurationOptions {
            token: Some("unseen".into()),
            team_id: Some("unseen".into()),
            ..Default::default()
        };

        let builder = TurborepoConfigBuilder {
            repo_root,
            override_config,
            global_config_path: Some(global_config_path),
            environment: env,
        };

        let config = builder.build().unwrap();
        assert_eq!(config.team_id().unwrap(), vercel_artifacts_owner);
        assert_eq!(config.token().unwrap(), vercel_artifacts_token);
        assert_eq!(config.spaces_id().unwrap(), "my-spaces-id");
    }

    #[test]
    fn test_turbo_json_remote_cache() {
        let tmp_dir = TempDir::new().unwrap();
        let repo_root = AbsoluteSystemPathBuf::try_from(tmp_dir.path()).unwrap();

        let api_url = "url1";
        let login_url = "url2";
        let team_slug = "my-slug";
        let team_id = "an-id";
        let turbo_json_contents = serde_json::to_string_pretty(&serde_json::json!({
            "remoteCache": {
                "enabled": true,
                "apiUrl": api_url,
                "loginUrl": login_url,
                "teamSlug": team_slug,
                "teamId": team_id,
                "signature": true,
                "preflight": false,
                "timeout": 123
            }
        }))
        .unwrap();
        repo_root
            .join_component("turbo.json")
            .create_with_contents(&turbo_json_contents)
            .unwrap();

        let builder = TurborepoConfigBuilder {
            repo_root,
            override_config: ConfigurationOptions::default(),
            global_config_path: None,
            environment: HashMap::default(),
        };

        let config = builder.build().unwrap();
        // Directly accessing field to make sure we're not getting the default value
        assert_eq!(config.enabled, Some(true));
        assert_eq!(config.api_url(), api_url);
        assert_eq!(config.login_url(), login_url);
        assert_eq!(config.team_slug(), Some(team_slug));
        assert_eq!(config.team_id(), Some(team_id));
        assert!(config.signature());
        assert!(!config.preflight());
        assert_eq!(config.timeout(), 123);
    }
}
