#!/usr/bin/python

from ansible.module_utils.basic import AnsibleModule
from ansible.module_utils.urls import fetch_url
from ansible.module_utils._text import to_text

import json
import os

DOCUMENTATION = r"""
---
module: pkg_release
short_description: Manage GitHub releases and upload assets
description:
  - This module creates or updates a GitHub release and uploads binary assets to it.
version_added: "1.0.2"
options:
  github_token:
    description:
      - Personal access token for GitHub authentication.
    required: false
    type: str
  repo:
    description:
      - GitHub repository in the format C(owner/repo).
    required: true
    type: str
  tag_name:
    description:
      - The name of the tag for the release.
    required: true
    type: str
  release_name:
    description:
      - The name of the release.
    required: true
    type: str
  release_description:
    description:
      - Description of the release.
    required: false
    type: str
    default: ''
  prerelease:
    description:
      - Whether the release is a prerelease.
    required: false
    type: bool
    default: false
  binaries:
    description:
      - List of binaries to upload. Each item can be a string (path) or a dictionary with C(path) and optional C(name).
    required: true
    type: list
    elements: raw
author:
  - Your Name (@yourhandle)
"""

EXAMPLES = r"""
- name: Create or update GitHub release and upload binaries
  pkg_release:
    github_token: "{{ lookup('env', 'GITHUB_PAT') }}"
    repo: evgnomon/zygote
    tag_name: v0.0.1
    release_name: Release v0.0.1
    release_description: Zygote release v0.0.1
    prerelease: false
    binaries:
      - path: /path/to/zygote
      - path: /path/to/another_binary
        name: custom_name_for_another_binary
"""

RETURN = r"""
changed:
  description: Whether any changes were made.
  type: bool
message:
  description: A message describing the result.
  type: str
release:
  description: Information about the created or updated release.
  type: dict
assets:
  description: Information about the uploaded assets.
  type: list
"""


def handle_response(response, info, module, success_status_codes=[200]):
    status_code = info["status"]
    if status_code not in success_status_codes:
        body = response.read() if response else ""
        try:
            error_response = json.loads(body)
        except Exception:
            error_response = to_text(body)
        module.fail_json(
            msg=f"Request failed. Status code: {status_code}",
            response=error_response,
            info=info,
        )
    else:
        body = response.read()
        try:
            return json.loads(body)
        except Exception:
            return body


def main():
    argument_spec = dict(
        github_token=dict(type="str", no_log=True),
        repo=dict(type="str", required=True),
        tag_name=dict(type="str", required=True),
        release_name=dict(type="str", required=True),
        release_description=dict(type="str", required=False, default=""),
        prerelease=dict(type="bool", default=False),
        binaries=dict(type="list", elements="raw", required=True),
    )

    module = AnsibleModule(
        argument_spec=argument_spec,
        supports_check_mode=True,
    )

    github_token = module.params["github_token"] or os.getenv("GITHUB_PAT")
    if not github_token:
        module.fail_json(msg="GitHub token is required")

    repo = module.params["repo"]
    tag_name = module.params["tag_name"]
    release_name = module.params["release_name"]
    release_description = module.params["release_description"]
    prerelease = module.params["prerelease"]
    binaries_input = module.params["binaries"]

    # Process binaries input
    binaries = []
    for item in binaries_input:
        if isinstance(item, dict):
            if "path" not in item:
                module.fail_json(msg="Each binary dict must have a 'path' key.")
            path = item["path"]
            name = item.get("name", os.path.basename(path))
        else:
            # Assume item is a string (path)
            path = item
            name = os.path.basename(path)
        binaries.append({"path": path, "name": name})

    release_url = f"https://api.github.com/repos/{repo}/releases"
    headers = {
        "Authorization": f"token {github_token}",
        "Accept": "application/vnd.github.v3+json",
    }

    result = dict(
        changed=False,
        message="",
        assets=[],
    )

    try:
        # Fetch existing releases
        response, info = fetch_url(module, release_url, headers=headers)
        releases = handle_response(response, info, module)

        # Check for existing release
        existing_release = next(
            (r for r in releases if r["tag_name"] == tag_name), None
        )

        if existing_release:
            release = existing_release
            # Get existing assets
            assets_url = release["assets_url"]
            response, info = fetch_url(module, assets_url, headers=headers)
            existing_assets = handle_response(response, info, module)
            existing_asset_names = [asset["name"] for asset in existing_assets]

            assets_to_upload = []
            for binary in binaries:
                if binary["name"] not in existing_asset_names:
                    assets_to_upload.append(binary)

            if assets_to_upload:
                if module.check_mode:
                    result["changed"] = True
                    module.exit_json(**result)

                # Upload assets
                upload_url = release["upload_url"].replace("{?name,label}", "")
                uploaded_assets = []
                for binary in assets_to_upload:
                    binary_file_path = binary["path"]
                    binary_filename = binary["name"]
                    upload_url_with_params = f"{upload_url}?name={binary_filename}"
                    with open(binary_file_path, "rb") as binary_file:
                        binary_data = binary_file.read()
                    upload_headers = headers.copy()
                    upload_headers["Content-Type"] = "application/octet-stream"
                    response, info = fetch_url(
                        module,
                        upload_url_with_params,
                        data=binary_data,
                        headers=upload_headers,
                        method="POST",
                        timeout=300,
                    )
                    upload_response = handle_response(
                        response, info, module, success_status_codes=[201]
                    )
                    uploaded_assets.append(upload_response)
                result["changed"] = True
                result["message"] = "Assets uploaded successfully."
                result["assets"] = uploaded_assets
            else:
                result["message"] = (
                    "All binaries already exist in the release. No changes made."
                )
        else:
            if module.check_mode:
                result["changed"] = True
                module.exit_json(**result)
            # Create new release
            release_data = {
                "tag_name": tag_name,
                "name": release_name,
                "body": release_description,
                "prerelease": prerelease,
            }
            data = json.dumps(release_data).encode("utf-8")
            response, info = fetch_url(
                module, release_url, data=data, headers=headers, method="POST"
            )
            release = handle_response(
                response, info, module, success_status_codes=[201]
            )

            # Upload assets
            upload_url = release["upload_url"].replace("{?name,label}", "")
            uploaded_assets = []
            for binary in binaries:
                binary_file_path = binary["path"]
                binary_filename = binary["name"]
                upload_url_with_params = f"{upload_url}?name={binary_filename}"
                with open(binary_file_path, "rb") as binary_file:
                    binary_data = binary_file.read()
                upload_headers = headers.copy()
                upload_headers["Content-Type"] = "application/octet-stream"
                response, info = fetch_url(
                    module,
                    upload_url_with_params,
                    data=binary_data,
                    headers=upload_headers,
                    method="POST",
                    timeout=300,
                )
                upload_response = handle_response(
                    response, info, module, success_status_codes=[201]
                )
                uploaded_assets.append(upload_response)
            result["changed"] = True
            result["message"] = "Release created and assets uploaded successfully."
            result["release"] = release
            result["assets"] = uploaded_assets

    except Exception as e:
        module.fail_json(msg=to_text(e))

    module.exit_json(**result)


if __name__ == "__main__":
    main()
