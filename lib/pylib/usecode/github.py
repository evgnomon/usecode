from dataclasses import dataclass
import json
import os
from typing import List, Optional


@dataclass
class AuthorCommitter:
    email: str
    name: str
    username: str


@dataclass
class Commit:
    author: AuthorCommitter
    committer: AuthorCommitter
    distinct: bool
    id: str
    message: str
    timestamp: str
    tree_id: str
    url: str


@dataclass
class Owner:
    avatar_url: str
    events_url: str
    followers_url: str
    following_url: str
    gists_url: str
    gravatar_id: str
    html_url: str
    id: int
    login: str
    node_id: str
    organizations_url: str
    received_events_url: str
    repos_url: str
    site_admin: bool
    starred_url: str
    subscriptions_url: str
    type: str
    url: str


@dataclass
class Organization:
    avatar_url: str
    description: str
    events_url: str
    hooks_url: str
    id: int
    issues_url: str
    login: str
    members_url: str
    node_id: str
    public_members_url: str
    repos_url: str
    url: str


@dataclass
class Pusher:
    email: str
    name: str


@dataclass
class Repository:
    allow_forking: bool
    archive_url: str
    archived: bool
    assignees_url: str
    blobs_url: str
    branches_url: str
    clone_url: str
    collaborators_url: str
    comments_url: str
    commits_url: str
    compare_url: str
    contents_url: str
    contributors_url: str
    created_at: str
    custom_properties: dict
    default_branch: str
    deployments_url: str
    description: str
    disabled: bool
    downloads_url: str
    events_url: str
    fork: bool
    forks: int
    forks_count: int
    forks_url: str
    full_name: str
    git_commits_url: str
    git_refs_url: str
    git_tags_url: str
    git_url: str
    has_discussions: bool
    has_downloads: bool
    has_issues: bool
    has_pages: bool
    has_projects: bool
    has_wiki: bool
    homepage: str
    hooks_url: str
    html_url: str
    id: int
    is_template: bool
    issue_comment_url: str
    issue_events_url: str
    issues_url: str
    keys_url: str
    labels_url: str
    language: Optional[str]
    languages_url: str
    license: Optional[str]
    merges_url: str
    milestones_url: str
    mirror_url: Optional[str]
    name: str
    node_id: str
    notifications_url: str
    open_issues: int
    open_issues_count: int
    owner: Owner
    private: bool
    pulls_url: str
    pushed_at: str
    releases_url: str
    size: int
    ssh_url: str
    stargazers_count: int
    stargazers_url: str
    statuses_url: str
    subscribers_url: str
    subscription_url: str
    svn_url: str
    tags_url: str
    teams_url: str
    topics: List[str]
    trees_url: str
    updated_at: str
    url: str
    visibility: str
    watchers: int
    watchers_count: int
    web_commit_signoff_required: bool


@dataclass
class Sender:
    avatar_url: str
    events_url: str
    followers_url: str
    following_url: str
    gists_url: str
    gravatar_id: str
    html_url: str
    id: int
    login: str
    node_id: str
    organizations_url: str
    received_events_url: str
    repos_url: str
    site_admin: bool
    starred_url: str
    subscriptions_url: str
    type: str
    url: str


@dataclass
class PushEvent:
    after: str
    base_ref: Optional[str]
    before: str
    commits: List[Commit]
    compare: str
    created: bool
    deleted: bool
    forced: bool
    head_commit: Commit
    pusher: Pusher
    ref: str
    repository: Repository
    sender: Sender
    organization: Optional[Organization] = None


@dataclass
class DeleteEvent:
    pusher_type: str
    ref: Optional[str]
    ref_type: str
    repository: Repository
    sender: Sender
    organization: Optional[Organization] = None


@dataclass
class GithubEnvVars:
    _github_event_path = os.getenv("GITHUB_EVENT_PATH")
    _github_event_name = os.getenv("GITHUB_EVENT_NAME")

    def event_path(self):
        if not self._github_event_path:
            raise Exception("GITHUB_EVENT_PATH is not set")
        return self._github_event_path

    def is_push_event(self):
        _yacht_event_name = os.getenv("YACHT_EVENT_NAME")
        if _yacht_event_name:
            return _yacht_event_name == "push"
        if not self._github_event_name:
            raise Exception("GITHUB_EVENT_NAME is not set")
        return self._github_event_name == "push"

    def is_delete_event(self):
        _yacht_event_name = os.getenv("YACHT_EVENT_NAME")
        if _yacht_event_name:
            return _yacht_event_name == "delete"
        if not self._github_event_name:
            raise Exception("GITHUB_EVENT_NAME is not set")
        return self._github_event_name == "delete"

    def ref_name(self) -> str:
        yacht_ref_name = os.getenv("YACHT_REF_NAME")
        if yacht_ref_name:
            return yacht_ref_name

        event: DeleteEvent | PushEvent | None = None
        with open(self.event_path()) as f:
            data = json.load(f)
            if self.is_push_event():
                event = PushEvent(**data)
            elif self.is_delete_event():
                event = DeleteEvent(**data)

        if not event:
            raise Exception("Github Event not found")

        if not event.ref:
            raise Exception("ref not found in event")

        return event.ref
