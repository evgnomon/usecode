import base64
from dataclasses import dataclass
from http import HTTPStatus

from ansible.module_utils.basic import AnsibleModule
from nacl.public import PublicKey, SealedBox
import requests


def fetch_public_key(owner: str, repo: str, token: str) -> tuple[str, str]:
    url = f"https://api.github.com/repos/{owner}/{repo}/actions/secrets/public-key"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json",
    }
    response = requests.get(url, headers=headers, timeout=10)
    response.raise_for_status()
    return response.json()["key"], response.json()["key_id"]


def encrypt_secret(public_key_b64: str, secret: str) -> str:
    public_key_bytes = base64.b64decode(public_key_b64)
    public_key = PublicKey(public_key_bytes)
    sealed_box = SealedBox(public_key)
    encrypted = sealed_box.encrypt(secret.encode())
    return base64.b64encode(encrypted).decode("utf-8")


@dataclass
class GithubSecret:
    owner_name: str
    repo_name: str
    secret_name: str
    encrypted_secret: str
    key_id: str


def set_secret(
    github_secret: GithubSecret,
    github_pat: str,
) -> None:
    url = f"https://api.github.com/repos/{github_secret.owner_name}/{github_secret.repo_name}/actions/secrets/{github_secret.secret_name}"
    headers = {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {github_pat}",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    data = {
        "encrypted_value": github_secret.encrypted_secret,
        "key_id": github_secret.key_id,
    }

    response = requests.put(url, headers=headers, json=data, timeout=10)
    response.raise_for_status()


def delete_github_secret(owner: str, repo: str, secret_name: str, token: str) -> None:
    """
    Delete a secret from a GitHub repository.

    Args:
    ----
    owner (str): The owner of the repository.
    repo (str): The repository name.
    secret_name (str): The name of the secret to delete.
    token (str): The GitHub personal access token.

    """
    url = f"https://api.github.com/repos/{owner}/{repo}/actions/secrets/{secret_name}"
    headers = {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {token}",
        "X-GitHub-Api-Version": "2022-11-28",
    }

    response = requests.delete(url, headers=headers, timeout=10)

    # 404 is OK:
    if response.status_code == HTTPStatus.NOT_FOUND:
        return

    response.raise_for_status()


def main() -> None:
    module_args = {
        "owner": {"type": "str", "required": True},
        "repo": {"type": "str", "required": True},
        "token": {"type": "str", "required": True, "no_log": True},
        "secret": {"type": "str", "required": True, "no_log": True},
        "secret_name": {"type": "str", "required": True},
        "state": {
            "type": "str",
            "choices": ["present", "absent"],
            "default": "present",
        },
    }

    module = AnsibleModule(argument_spec=module_args, supports_check_mode=True)

    result = {"changed": False, "message": ""}

    if module.check_mode:
        module.exit_json(**result)

    try:
        if module.params["state"] == "absent":
            delete_github_secret(
                module.params["owner"],
                module.params["repo"],
                module.params["secret_name"],
                module.params["token"],
            )

        if not module.params["state"] == "present":
            raise Exception(f"Invalid state: {module.params['state']}")  # noqa: TRY301

        public_key, key_id = fetch_public_key(
            module.params["owner"],
            module.params["repo"],
            module.params["token"],
        )
        encrypted_secret = encrypt_secret(public_key, module.params["secret"])
        result["encrypted_secret"] = encrypted_secret
        result["key_id"] = key_id
        github_secret = GithubSecret(
            owner_name=module.params["owner"],
            repo_name=module.params["repo"],
            secret_name=module.params["secret_name"],
            encrypted_secret=encrypted_secret,
            key_id=key_id,
        )
        set_secret(
            github_secret,
            module.params["token"],
        )

    except Exception as e:
        module.fail_json(msg=e, **result)

    module.exit_json(**result)


if __name__ == "__main__":
    main()
