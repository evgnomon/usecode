#!/usr/bin/python

import asyncio
import os
import platform
import docker
from docker.errors import APIError
from ansible.module_utils.basic import AnsibleModule
from catamaran.github import GithubEnvVars
from catamaran.ansible import AnsibleResult
from catamaran import delete_image

# Documentation for Ansible Galaxy
DOCUMENTATION = """
---
module: gh_image
short_description: Manage Github Container Registry images
description:
  - Build, push, or delete Docker images in Github Container Registry
options:
  image:
    description:
      - Name of the image
    required: true
    type: str
  owner:
    description:
      - Owner/organization of the image
    required: true
    type: str
  tag:
    description:
      - Tag of the image
    required: false
    type: str
  state:
    description:
      - Desired state of the image ('present' or 'absent')
    required: false
    type: str
    default: present
    choices: ['present', 'absent']
  token:
    description:
      - Github token for authentication
    required: true
    type: str
  dockerfile:
    description:
      - Path to Dockerfile
    required: false
    type: str
    default: Dockerfile
  context:
    description:
      - Build context path
    required: false
    type: str
    default: .
author:
  - Hamed Ghasemzadeh (hg@evgnomon.org)
"""
EXAMPLES = """
- name: Build and push an image
  evgnomon.catamaran.gh_image:
    image: "my-image"
    owner: "my-owner"
    tag: "latest"
    state: "present"
    token: "my-token"
    dockerfile: "Dockerfile"
    context: "."

- name: Delete an image
  evgnomon.catamaran.gh_image:
    image: "my-image"
    owner: "my-owner"
    tag: "latest"
    state: "absent"
    token: "my-token"
"""
RETURN = """
result:
  description: Result of the operation
  type: dict
  returned: always
  contains:
    changed:
      description: Whether the module made changes
      type: bool
    msg:
      description: Status message
      type: str
    skipped:
      description: Whether the operation was skipped
      type: bool
"""


async def run_module():
    module_args = dict(
        image=dict(type="str", required=True),
        user=dict(type="str", required=False),
        owner=dict(type="str", required=True),
        tag=dict(type="str", required=True),
        state=dict(type="str", default="present", choices=["present", "absent"]),
        token=dict(type="str", required=True, no_log=True),
        publish=dict(type="str", required=False, no_log=True),
        dockerfile=dict(type="str", default="Dockerfile"),
        context=dict(type="str", default="."),
    )

    result = AnsibleResult()
    env_vars = GithubEnvVars()

    module = AnsibleModule(argument_spec=module_args, supports_check_mode=True)

    image_name = module.params["image"]
    owner = module.params["owner"]
    tag = module.params.get("tag")
    state = module.params["state"]
    token = module.params["token"]
    dockerfile = module.params["dockerfile"]
    context = module.params["context"]
    actor = module.params.get("user", owner)

    tag = tag.replace("/", "-")

    docker_sock = os.getenv("DOCKER_SOCK", get_docker_socket())
    docker_client = docker.APIClient(base_url=docker_sock)
    try:
        # Initialize Docker client with Unix socket

        # Construct full image name
        full_image_name = f"ghcr.io/{owner}/{image_name}:{tag}"

        if state == "present":
            if module.check_mode:
                result.msg = f"Would build and push image {full_image_name}"
                module.exit_json(**result.to_dict())

            # Login to GitHub Container Registry
            try:
                docker_client.login(username=actor, password=token, registry="ghcr.io")
            except APIError as e:
                module.fail_json(
                    msg=f"Failed to login to ghcr.io: {str(e)}. Ensure the token has correct permissions."
                )

            # Build Docker image
            try:
                result.msg = f"Building image {full_image_name}"
                build_logs = docker_client.build(
                    path=context,
                    dockerfile=dockerfile,
                    tag=full_image_name,
                    rm=True,
                    pull=True,
                    decode=True,
                )
                for line in build_logs:
                    if "error" in line:
                        module.fail_json(msg=f"Build error: {line.get('error')}")
                result.changed = True
            except APIError as e:
                module.fail_json(msg=f"Failed to build image: {str(e)}")

            # Push image to GitHub Packages
            if module.params.get("publish"):
                try:
                    result.msg = f"Pushing image {full_image_name}"
                    push_logs = docker_client.push(
                        repository=f"ghcr.io/{owner}/{image_name}",
                        tag=tag,
                        stream=True,
                        decode=True,
                    )
                    for line in push_logs:
                        if "error" in line:
                            module.fail_json(msg=f"Push error: {line.get('error')}")
                    result.changed = True
                    result.msg = f"Successfully built and pushed {full_image_name}"
                except APIError as e:
                    module.fail_json(
                        msg=f"Failed to push image: {str(e)}. Ensure the token has 'write:packages' scope."
                    )

        elif state == "absent":
            if not env_vars.is_delete_event():
                result.msg = "Event is not a ref delete event"
                result.skipped = True
                module.exit_json(**result.to_dict())

            if not tag:
                tag = env_vars.ref_name()

            if module.check_mode:
                result.msg = f"Would delete image {full_image_name}"
                module.exit_json(**result.to_dict())

            await delete_image(
                tag,
                image_name,
                actor,
                token=token,
            )
            result.msg = f"Image {full_image_name} deleted."
            result.changed = True

        module.exit_json(**result.to_dict())

    except Exception as e:
        module.fail_json(msg=f"Error: {str(e)}")
    finally:
        if docker_client is not None:
            docker_client.close()


async def main():
    await run_module()


def get_docker_socket():
    system = platform.system().lower()
    if system == "windows":
        return "npipe:////./pipe/docker_engine"
    elif system == "linux":
        return "unix:///var/run/docker.sock"
    elif system == "darwin":
        home_path = os.getenv("HOME")
        return f"unix://{home_path}/.docker/run/docker.sock"
    else:
        raise RuntimeError(f"Unsupported OS: {system}")


if __name__ == "__main__":
    asyncio.run(main())
