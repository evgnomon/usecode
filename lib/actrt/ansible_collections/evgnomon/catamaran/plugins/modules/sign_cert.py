#!/usr/bin/python

import socket
from ansible.module_utils.basic import AnsibleModule

DOCUMENTATION = r"""
---
module: sign_cert
short_description: Generate and execute certificate signing command for a specific shard and replica combination
description:
  - This module generates and executes a certificate signing command for a specific shard and replica combination.
  - Commands are executed using the `z cert sign` tool, which must be available on the target system.
  - The certificate is generated with associated domain and IP addresses for the specified shard/replica.
  - The shard and replica parameters are provided as strings (no numeric conversion needed).
version_added: "1.0.0"
options:
  domain:
    description:
      - The base domain name used for generating the subdomain.
      - For instance, 'example.com' results in subdomains like 'shard-a.example.com'.
    required: true
    type: str
  token:
    description:
      - A unique token used in certificate names for identification.
      - For instance, 'abc123' results in certificate names like 'zygote-abc123-a'.
    required: true
    type: str
  shard:
    description:
      - The shard identifier to generate certificate for.
      - Use "main", "0", or empty string for main shard certificate (without replica suffix).
      - Use any other string for replica certificates with that suffix.
      - Must be a string.
    required: true
    type: str
  replica:
    description:
      - The replica identifier (already in letter format).
      - This will be used as the base identifier for the certificate and domain names.
      - Must be a non-empty string.
    required: true
    type: str
  tenant:
    description:
      - The tenant name used as a prefix for certificate names.
      - If not specified, defaults to 'zygote'.
      - For instance, 'mytenant' results in certificate names like 'mytenant-abc123-a'.
    required: false
    type: str
    default: zygote
  node_type:
    description:
      - The node type used in subdomain generation.
      - If not specified, defaults to 'shard'.
      - For instance, 'node' results in subdomains like 'node-a.example.com'.
    required: false
    type: str
    default: shard
author:
  - Hamed Ghasemzadeh (hg@evgnomon.org)
notes:
  - This module executes commands using the `z cert sign` tool. Ensure it is installed and accessible on the target system.
  - Domain names are resolved to IP addresses using Python's socket module for DNS resolution.
  - In check mode, the module returns the command without executing it.
  - If the command fails (non-zero return code), the module fails.
requirements:
  - python >= 3.6
  - z cert sign tool
seealso:
  - name: Ansible command module
    description: Details on how commands are executed
    link: https://docs.ansible.com/ansible/latest/collections/ansible/builtin/command_module.html
"""

EXAMPLES = r"""
# Generate certificate for replica set 'a', main shard
- name: Generate certificate for main shard
  evgnomon.catamaran.sign_cert:
    domain: example.com
    token: abc123
    replica: a
    shard: main
  register: cert_result

# Generate certificate for replica set 'a', shard 0 (also main shard)
- name: Generate certificate for shard 0 (main shard)
  evgnomon.catamaran.sign_cert:
    domain: example.com
    token: abc123
    replica: a
    shard: "0"
  register: cert_result

# Generate certificate for replica set 'a', first replica
- name: Generate certificate for first replica of replica set 'a'
  evgnomon.catamaran.sign_cert:
    domain: example.com
    token: abc123
    replica: a
    shard: "1"
  register: cert_result

# Generate certificate with custom tenant and node type
- name: Generate certificate with custom settings
  evgnomon.catamaran.sign_cert:
    domain: example.com
    token: abc123
    replica: b
    shard: "2"
    tenant: mytenant
    node_type: node
  register: cert_result

# Run in check mode to preview command without execution
- name: Preview certificate command
  evgnomon.catamaran.sign_cert:
    domain: example.com
    token: abc123
    replica: a
    shard: main
  check_mode: yes
  register: cert_result
"""

RETURN = r"""
command:
  description: The certificate signing command that was generated and (if not in check mode) executed.
  type: str
  returned: always
  sample: "z cert sign --name zygote-abc123-a --name shard-a.example.com --ip 127.0.0.1 --ip 192.168.1.100"
result:
  description: A dictionary containing the execution result of the command (empty in check mode).
  type: dict
  returned: always
  contains:
    cmd:
      description: The command that was executed.
      type: str
    rc:
      description: The return code of the command (0 indicates success).
      type: int
    stdout:
      description: The standard output of the command.
      type: str
    stderr:
      description: The standard error of the command.
      type: str
  sample:
    cmd: "z cert sign --name zygote-abc123-a --name shard-a.example.com --ip 127.0.0.1 --ip 192.168.1.1"
    rc: 0
    stdout: "Certificate signed successfully"
    stderr: ""
changed:
  description: Indicates if the module made changes by executing the command (always true unless in check mode).
  type: bool
  returned: always
  sample: true
msg:
  description: Error message if the module fails.
  type: str
  returned: on failure
  sample: "Failed to execute command: z cert sign ... (rc=1)"
"""


def resolve_domain_ip(domain):
    """Resolve domain name to IP address using Python's socket module"""
    try:
        # Get all IP addresses for the domain
        ip_addresses = socket.getaddrinfo(
            domain, None, socket.AF_INET, socket.SOCK_STREAM
        )
        # Return the first IPv4 address
        for addr_info in ip_addresses:
            if addr_info[0] == socket.AF_INET:
                return addr_info[4][0]
        # If no IPv4 found, return the first available
        if ip_addresses:
            return ip_addresses[0][4][0]
        else:
            raise ValueError(f"Unable to resolve domain '{domain}' to any IP address")
    except socket.gaierror as e:
        raise ValueError(f"DNS resolution failed for domain '{domain}': {e}")
    except Exception as e:
        raise ValueError(f"Error resolving domain '{domain}': {e}")


def run_module():
    # Define module arguments
    module_args = dict(
        domain=dict(type="str", required=True),
        token=dict(type="str", required=False),
        shard=dict(type="str", required=True),
        replica=dict(type="str", required=True),
        tenant=dict(type="str", required=False, default="zygote"),
        node_type=dict(type="str", required=False, default="shard"),
    )

    # Initialize result dictionary
    result = dict(changed=False, command="", result=dict(), msg="")

    # Initialize Ansible module
    module = AnsibleModule(argument_spec=module_args, supports_check_mode=True)

    # Get parameters
    domain = module.params["domain"]
    token = module.params["token"]
    shard = module.params["shard"]
    replica = module.params["replica"]
    tenant = module.params.get("tenant", "zygote")
    node_type = module.params.get("node_type", "shard")

    # Validate inputs
    if not shard or not isinstance(shard, str):
        result["msg"] = "shard must be a non-empty string"
        module.fail_json(**result)

    if not replica or not isinstance(replica, str):
        result["msg"] = "replica must be a non-empty string"
        module.fail_json(**result)

    try:
        # Use the provided replica directly (no conversion needed)
        # Generate certificate name and domain based on shard parameter
        if shard == "main" or shard == "" or shard == "0":
            # Main shard certificate (no replica suffix)
            cert_name = f"{tenant}-{node_type}-{token}-{replica}.{domain}"
            short_name = f"{node_type}-{token}-{replica}"
            cert_domain = f"{node_type}-{replica}.{domain}"
            if token is None or token == "":
                cert_name = f"{node_type}-{replica}.{domain}"
                short_name = f"{node_type}-{replica}"
        else:
            # Replica certificate with shard suffix
            cert_name = f"{tenant}-{node_type}-{token}-{replica}-{shard}.{domain}"
            short_name = f"{node_type}-{token}-{replica}-{shard}"
            cert_domain = f"{node_type}-{replica}-{shard}.{domain}"
            if token is None or token == "":
                cert_name = f"{node_type}-{replica}-{shard}.{domain}"
                short_name = f"{node_type}-{replica}-{shard}"

        # Resolve IP address
        try:
            resolved_ip = resolve_domain_ip(cert_domain)
        except ValueError as e:
            result["msg"] = f"Failed to resolve IP for domain {cert_domain}: {e}"
            module.fail_json(**result)

        # Generate the certificate signing command
        command = (
            f"z cert sign --name {cert_name} --name {cert_domain} --name {short_name} "
            f"--ip 127.0.0.1 --ip {resolved_ip}"
        )
        result["command"] = command

        # Set changed to True since command would be generated/executed
        result["changed"] = True

        # In check mode, return without executing
        if module.check_mode:
            module.exit_json(**result)

        # Execute the command
        rc, stdout, stderr = module.run_command(command, use_unsafe_shell=True)
        result["result"] = {
            "cmd": command,
            "rc": rc,
            "stdout": stdout,
            "stderr": stderr,
        }

        if rc != 0:
            result["msg"] = (
                f"Failed to execute command: {command} (rc={rc}, stderr={stderr})"
            )
            module.fail_json(**result)

        # Exit with success
        module.exit_json(**result)

    except Exception as e:
        result["msg"] = f"Error generating or executing certificate command: {str(e)}"
        module.fail_json(**result)


def main():
    run_module()


if __name__ == "__main__":
    main()
