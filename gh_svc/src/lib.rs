use std::convert::Into;
use std::env::VarError;
// use std::future::Future;
use std::sync::Arc;

use anyhow::Context;
// use futures::{StreamExt, TryFutureExt};
use futures::future::try_join_all;
use octocrab::models::{Contributor, Repository};
use octocrab::{Error, Octocrab, Page, Result};
use rayon::prelude::*;
use serde::Serialize;
use tracing::info;

#[derive(Clone, Default, Debug)]
pub struct OctocatParametersDto {
    pub owner: String,
    pub start_from_page: u8,
    pub repo_owner: String,
    pub repo_name: String,
}

#[derive(Serialize, Debug)]
pub struct GHContributorResult {
    name: String,
    contributions: u32,
}

impl From<Contributor> for GHContributorResult {
    fn from(value: Contributor) -> Self {
        GHContributorResult {
            name: value.author.login,
            contributions: value.contributions,
        }
    }
}

#[derive(Debug)]
pub struct GithubService {
    octocrab: Arc<octocrab::Octocrab>,
    per_page_size: u8,
}

impl GithubService {
    /// per_page_size is used in all github requests (default value is 30), here set at max (100)
    pub fn new(token: std::result::Result<String, VarError>) -> Self {
        Self {
            octocrab: Arc::new(
                match token {
                    Ok(tkn) => Octocrab::builder().personal_token(tkn),
                    Err(_) => Octocrab::builder(),
                }
                .build()
                .unwrap(),
            ),
            per_page_size: 100,
        }
    }

    async fn __example_get_org_repos_v2(
        &self,
        owner: String,
        start_from_page: u8,
    ) -> Result<Vec<Repository>> {
        let page = self
            .octocrab
            .orgs(owner.to_string())
            .list_repos()
            .per_page(self.per_page_size)
            .page(start_from_page)
            .send()
            .await
            .unwrap();
        self.octocrab.all_pages(page).await
    }

    async fn __example_get_repo_contributors_v2(
        &self,
        repo_owner: String,
        repo_name: String,
        start_from_page: u8,
    ) -> Result<Vec<Contributor>> {
        let page = self
            .octocrab
            .repos(repo_owner, repo_name)
            .list_contributors()
            .per_page(self.per_page_size)
            .page(start_from_page)
            .send()
            .await
            .unwrap();
        self.octocrab.all_pages(page).await
    }

    #[tracing::instrument(level = "debug"/*, ret*/)]
    pub async fn get_repos_with_contributors_v2(
        &self,
        org_name: String,
    ) -> Result<Vec<GHContributorResult>, anyhow::Error> {
        info!("getting contributors for: {}", org_name);
        let get_repos = self
            .get_repos(org_name.clone())
            .await
            .with_context(|| format!("Unable to fetch {} repositories", &org_name));
        if get_repos.is_err() {
            return Err(get_repos.unwrap_err());
        }
        let try_repos = get_repos?;

        let repos_with_owners: Vec<(String, String)> = try_repos
            .par_iter()
            .map(|repo| {
                (
                    repo.owner.as_ref().unwrap().login.clone(),
                    repo.name.clone(),
                )
            })
            .collect();

        let mut contributions = vec![];
        contributions.extend(repos_with_owners.iter().map(|repo_with_owner| {
            let (repo_owner, repo_name) = repo_with_owner.clone();
            self.get_repo_contributors(repo_owner, repo_name)
            // .map_ok(|rc| GHContributorResult::from(rc))
        }));

        let maybe_contributors1 = try_join_all(contributions)
            .await?
            .into_par_iter()
            .flatten()
            .collect::<Vec<Contributor>>();

        let mut maybe_contributors = maybe_contributors1
            .into_par_iter()
            .map(|c| c.into())
            .collect::<Vec<GHContributorResult>>();
        maybe_contributors.par_sort_by(|elm1, elm2| elm2.contributions.cmp(&elm1.contributions));
        Ok(maybe_contributors)
    }

    //////////////////////////////////
    //////////////////////////////////
    //////////////////////////////////
    async fn get_org_repos_internal(
        &self,
        owner: String,
        start_from_page: u8,
    ) -> Result<Page<Repository>> {
        // println!("get_org_repos_internal: {:?}", &owner);
        self.octocrab
            .orgs(owner.to_string())
            .list_repos()
            .per_page(self.per_page_size)
            .page(start_from_page)
            .send()
            .await
    }

    pub async fn get_repos(&self, owner: String) -> Result<Vec<Repository>, Error> {
        let mut results: Vec<Repository> = vec![];
        let mut start_from_page = 0;
        let mut has_more = true;
        let mut last_error: Option<Error> = None::<Error>;
        while has_more {
            let repos = self
                .get_org_repos_internal(owner.clone(), start_from_page)
                .await;
            match repos {
                Ok(page) => {
                    results.extend(page.items);
                    has_more = page.next.is_some();
                    start_from_page += 1;
                }
                Err(d) => {
                    last_error = Some(d);
                    has_more = false;
                }
            }
        }
        if let Some(le) = last_error {
            Err(le)
        } else {
            Ok(results)
        }
    }

    async fn get_repo_contributors_internal(
        &self,
        repo_owner: String,
        repo_name: String,
        start_from_page: u8,
    ) -> Result<Page<Contributor>> {
        self.octocrab
            .repos(repo_owner, repo_name)
            .list_contributors()
            .per_page(self.per_page_size)
            .page(start_from_page)
            .send()
            .await
    }

    pub async fn get_repo_contributors(
        &self,
        repo_owner: String,
        repo_name: String,
    ) -> Result<Vec<Contributor>, Error> {
        let mut results: Vec<Contributor> = vec![];
        let mut start_from_page = 0;
        let mut has_more = true;
        let mut last_error: Option<Error> = None::<Error>;
        while has_more {
            let contributors = self
                .get_repo_contributors_internal(
                    repo_owner.clone(),
                    repo_name.clone(),
                    start_from_page,
                )
                .await;
            match contributors {
                Ok(page) => {
                    results.extend(page.items);
                    has_more = page.next.is_some();
                    start_from_page += 1;
                }
                Err(d) => {
                    last_error = Some(d);
                    has_more = false;
                }
            }
        }
        if let Some(le) = last_error {
            Err(le)
        } else {
            Ok(results)
        }
    }
}

#[cfg(test)]
mod tests {
    // #[test]
    // fn it_works() {
    //     let result = add(2, 2);
    //     assert_eq!(result, 4);
    // }
}
