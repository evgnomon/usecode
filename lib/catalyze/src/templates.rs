// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! File templates written by catalyze (byte-identical to the original heredocs).

/// Content of `.github/workflows/yacht.yaml`.
pub const YACHT_WORKFLOW: &str = r#"name: Yacht

on:
  workflow_dispatch:
  push:
    branches:
      - '*'
    tags:
      - '*'
  delete:
  # schedule:
  #   - cron: '0 3 * * *'

jobs:
  yacht:
    runs-on: ubuntu-latest
    if: >-
      (github.event_name == 'push') ||
      (github.event_name == 'workflow_dispatch') ||
      (github.event_name == 'delete' &&
       github.event.ref_type == 'branch' &&
       github.event.ref != 'main')
    container:
      image: ghcr.io/evgnomon/barge:main
    permissions:
      contents: read
      packages: write
    steps:
      - uses: actions/checkout@v4
      - name: Run Playbook
        uses: evgnomon/yacht@main
        with:
          vault: ${{ secrets.VAULT_FILE }}
          vault_pass: ${{ secrets.VAULT_PASS }}
          github_token: ${{ secrets.GITHUB_TOKEN }}
"#;

/// Content of `playbooks/main.yaml` (written only when missing).
pub const MAIN_PLAYBOOK: &str = r#"- name: Build
  hosts: localhost
  gather_facts: false
  collections:
    - evgnomon.usecode
  roles:
    - role: z_secrets
    - role: z_defaults
    - role: z_build

- name: Publish Galaxy Collection
  hosts: localhost
  gather_facts: false
  collections:
    - evgnomon.usecode
  roles:
    - role: z_galaxy_col
"#;
