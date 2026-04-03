use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use futures::future::join_all;
use log::{debug, error, info, trace, warn};
use octocrab::{models::IssueState, Octocrab};
use tokio::{
    spawn,
    sync::{
        mpsc::{channel, Sender},
        RwLock,
    },
};

use crate::repo_config::WorktreeListing;

#[derive(Debug, Clone)]
pub struct PullRequest {
    pub pr_number: u64,
    pub review_status: ReviewStatus,
    pub check_status: CheckStatus,
    pub merge_status: MergStatus,
}

#[derive(Debug, Clone)]
pub enum CheckStatus {
    Success,
    Failed,
    InProgress,
}

#[derive(Debug, Clone)]
pub enum MergStatus {
    Pending,
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Clone)]
pub enum ReviewStatus {
    Pending,
    Approved,
    Blocked,
}

#[derive(Debug, Clone)]
pub struct AugmentedTree {
    pub branch_name: String,
    pub pr: Option<PullRequest>,
}

impl AugmentedTree {
    pub fn get_status_repr(&self) -> String {
        todo!("");
    }
}

#[derive(Clone, Debug)]
pub enum State {
    Loading,
    Failed(String),
    Ready(Vec<AugmentedTree>),
}

pub struct GitHub {
    client: Arc<Octocrab>,
    /// TODO: Make this make sense...
    /// `statuses[repo_index][worktree_index]`
    statuses: Arc<RwLock<HashMap<usize, State>>>,
    sender: Sender<(usize, State)>,
}

impl GitHub {
    /// Creates a new GitHub instance
    /// Fails if auth is not setup
    pub fn new(token: &str) -> Result<GitHub> {
        debug!("Creating new GitHub client");
        let client = Arc::new(Octocrab::builder().personal_token(token).build()?);
        let statuses = Arc::new(RwLock::new(HashMap::new()));
        let (tx, mut recv) = channel::<(usize, State)>(10);
        let gh = GitHub {
            client,
            statuses,
            sender: tx,
        };
        spawn({
            let statuses = gh.statuses.clone();
            async move {
                loop {
                    let res = recv.recv().await;
                    if let Some((index, new_state)) = res {
                        statuses.write().await.insert(index, new_state);
                    }
                }
            }
        });
        Ok(gh)
    }

    /// Called in CLI mode. Sets up device code auth flow.
    /// This function can be async, but since it is used in one place as part of cli mode, it is
    /// simpler to keey it sync.
    pub fn setup_auth() -> Result<String> {
        info!("Starting GitHub device auth flow");
        let client_id = "Ov23liH0a8VfNPCQutUW";

        trace!("Requesting the code for device auth flow");
        let resp = ureq::post("https://github.com/login/device/code")
            .header("Accept", "application/json")
            .send_form([("client_id", client_id), ("scope", "repo")])?
            .into_body()
            .read_to_string()?;
        debug!("Response from github: {resp}");
        let code: serde_json::Value = serde_json::from_str(&resp)?;

        let device_code = code["device_code"].as_str().unwrap().to_string();
        let user_code = code["user_code"].as_str().unwrap().to_string();
        let verification_uri = code["verification_uri"].as_str().unwrap().to_string();

        info!("Please visit {verification_uri} and enter code: {user_code}");

        loop {
            std::thread::sleep(std::time::Duration::from_secs(5));

            debug!("Polling for auth token...");
            let token_response: serde_json::Value = serde_json::from_reader(
                ureq::post("https://github.com/login/oauth/access_token")
                    .header("Accept", "application/json")
                    .send_form([
                        ("client_id", client_id),
                        ("device_code", device_code.as_str()),
                        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ])?
                    .into_body()
                    .into_reader(),
            )?;

            if let Some(error) = token_response["error"].as_str() {
                match error {
                    "authorization_pending" => {
                        trace!("Authorization still pending, will retry");
                        continue;
                    }
                    "slow_down" => {
                        warn!("Polling too fast, backing off...");
                        continue;
                    }
                    _ => {
                        error!(
                            "Auth failed: {} - {}",
                            error, token_response["error_description"]
                        );
                        anyhow::bail!(
                            "Auth failed: {} - {}",
                            error,
                            token_response["error_description"]
                        );
                    }
                }
            }

            if let Some(access_token) = token_response["access_token"].as_str() {
                info!("Successfully obtained access token");
                return Ok(access_token.to_owned());
            }
        }
    }

    /// Fetches PR statuses for all worktrees in a repository.
    ///
    /// Returns immediately with `State::Loading`. The actual state is updated
    /// asynchronously and delivered via an internal channel.
    ///
    /// # Arguments
    /// * `owner` - GitHub repository owner
    /// * `repo` - GitHub repository name
    /// * `tree_index` - Index used for caching state
    /// * `worktrees` - List of worktrees to fetch PR info for
    /// * `force_reload` - If true, skips cache and forces a reload
    ///
    /// # Caching Behavior
    /// Returns cached state if:
    /// - State exists and is not `Loading`, unless `force_reload` is true
    /// - If state is `Loading`, returns it regardless of `force_reload`
    pub async fn get_prs(
        &mut self,
        owner: String,
        repo: String,
        tree_index: usize,
        worktrees: Vec<WorktreeListing>,
        force_reload: bool,
    ) -> Result<State> {
        // If the State is already set, even if loading then return the state
        // If `force_reload` is true, then skip unless it is already loading
        if let Some(state) = self.statuses.read().await.get(&tree_index) {
            // Cannot use or statements due to let statements
            if let State::Loading = state {
                return Ok((*state).clone());
            } else if !force_reload {
                return Ok((*state).clone());
            }
        };

        self.statuses
            .write()
            .await
            .insert(tree_index, State::Loading);

        // Spawns a task that retrieves the PR states and sends the data to a consumer to update
        // the state. The state is returned at the top of this function
        spawn({
            // Need to clone here to avoid moving `self` into the closure
            let client = self.client.clone();
            let sender = self.sender.clone();
            async move {
                let tasks: Vec<_> = worktrees
            .iter()
            .map(|tree| {
                let client = client.clone();
                let owner = owner.clone();
                let repo = repo.clone();
                async move {
                    let pulls_handler = client.pulls(&owner, &repo);
                    let pr_req = pulls_handler.list().head(&tree.reference).send().await?;
                    let mut pr: Option<PullRequest> = None;
                    if let Some(c) = pr_req.total_count {
                        if c > 0 {
                            let pr_obj = pr_req.items[0].clone();

                            // Merge status
                            let merge_status: MergStatus;
                            if pr_obj.merged.is_some() && pr_obj.merged.unwrap() {
                                merge_status = MergStatus::Merged;
                            } else if pr_obj.state.is_some() {
                                if pr_obj.state.unwrap() == IssueState::Open {
                                    merge_status = MergStatus::Open;
                                } else {
                                    merge_status = MergStatus::Closed;
                                }
                            } else {
                                merge_status = MergStatus::Pending;
                            };

                            let reviews_handler = client.pulls(&owner, &repo);
                            let reviews =
                                reviews_handler.list_reviews(pr_obj.number).send().await?;
                            let review_status: ReviewStatus;
                            if reviews.total_count.is_some() {
                                if reviews.total_count.unwrap() == 0 {
                                    review_status = ReviewStatus::Approved;
                                } else if reviews.items.iter().any(|r| {
                                    r.state.is_some()
                                        && r.state.unwrap()
                                            == octocrab::models::pulls::ReviewState::Pending
                                }) {
                                    review_status = ReviewStatus::Pending;
                                } else if reviews.items.iter().any(|r| r.state.is_some() && r.state.unwrap() == octocrab::models::pulls::ReviewState::ChangesRequested) {
                                    review_status = ReviewStatus::Blocked;
                                } else {
                                    review_status = ReviewStatus::Approved;
                                }
                            } else {
                                review_status = ReviewStatus::Approved;
                            }
                            let checks_handler = client.checks(&owner, &repo);
                            let checks = checks_handler
                                .list_check_runs_for_git_ref(octocrab::params::repos::Commitish(
                                    pr_obj.head.sha,
                                ))
                                .send()
                                .await?;
                            let check_status: CheckStatus;
                            if checks.check_runs.iter().any(|c| {
                                c.conclusion.is_some()
                                    && c.conclusion.as_ref().unwrap() == "success"
                            }) {
                                check_status = CheckStatus::Failed;
                            } else if checks.check_runs.iter().any(|c| c.conclusion.is_none()) {
                                check_status = CheckStatus::InProgress;
                            } else {
                                check_status = CheckStatus::Success;
                            }

                            pr = Some(PullRequest {
                                pr_number: pr_obj.number,
                                check_status,
                                merge_status,
                                review_status,
                            })
                        }
                    }

                    return Ok(AugmentedTree {
                        branch_name: tree.reference.clone(),
                        pr,
                    });
                }
            })
            .collect();

                let res: Vec<Result<AugmentedTree>> = join_all(tasks).await;
                let errors: Vec<_> = res
                    .iter()
                    .filter(|e| Result::is_err(e))
                    .map(|e| e.as_ref().unwrap_err())
                    .collect();
                if errors.len() > 0 {
                    sender
                        .send((
                            tree_index,
                            State::Failed(
                                errors
                                    .iter()
                                    .map(|e| e.to_string())
                                    .reduce(|mut acc, s| {
                                        acc.push_str(&s);
                                        acc
                                    })
                                    .unwrap_or("".to_owned()),
                            ),
                        ))
                        .await
                        .unwrap();
                }

                let statuses: Vec<AugmentedTree> =
                    res.iter().map(|e| e.as_ref().unwrap().clone()).collect();
                sender
                    .send((tree_index, State::Ready(statuses)))
                    .await
                    .unwrap();
            }
        });
        return Ok(State::Loading);
    }
}
