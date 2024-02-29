use crate::Result;
use anyhow::anyhow;
use duct::cmd;
use std::path::Path;
use url::Url;

pub(crate) struct Git;
impl Git {
    pub(crate) fn clone(url: &Url, working_dir: &Path, branch: Option<&str>) -> Result<()> {
        // todo: use git dependency instead of command
        if !working_dir.exists() {
            let mut args = vec![
                "clone",
                "--depth",
                "1",
                url.as_str(),
                working_dir.to_str().expect("working directory is invalid"),
                "--quiet",
            ];
            if let Some(branch) = branch {
                args.push("--branch");
                args.push(branch);
            }
            let command = cmd("git", args);
            command.read()?;
        }
        Ok(())
    }
}

pub struct GitHub;
type Tag = String;
impl GitHub {
    pub async fn get_latest_release(repo: &Url) -> Result<Tag> {
        static APP_USER_AGENT: &str =
            concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

        let client = reqwest::ClientBuilder::new()
            .user_agent(APP_USER_AGENT)
            .build()?;
        let response = client
            .get(format!(
                "https://api.github.com/repos/{}/{}/releases/latest",
                Self::org(repo)?,
                Self::name(repo)?
            ))
            .send()
            .await?;
        let value = response.json::<serde_json::Value>().await?;
        value
            .get("tag_name")
            .and_then(|v| v.as_str())
            .map(|v| v.to_owned())
            .ok_or(anyhow!("the github release tag was not found"))
    }

    fn org(repo: &Url) -> Result<&str> {
        let path_segments = repo
            .path_segments()
            .map(|c| c.collect::<Vec<_>>())
            .expect("repository must have path segments");
        Ok(path_segments.get(0).ok_or(anyhow!(
            "the organization (or user) is missing from the github url"
        ))?)
    }

    pub(crate) fn name(repo: &Url) -> Result<&str> {
        let path_segments = repo
            .path_segments()
            .map(|c| c.collect::<Vec<_>>())
            .expect("repository must have path segments");
        Ok(path_segments.get(1).ok_or(anyhow!(
            "the repository name is missing from the github url"
        ))?)
    }

    pub(crate) fn release(repo: &Url, tag: &str, artifact: &str) -> String {
        format!("{}/releases/download/{tag}/{artifact}", repo.as_str())
    }
}
