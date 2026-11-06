# uc fs

Buckets and objects on the cluster's S3 storage (RustFS), with `cp`-like
paths: `<bucket>:<key>` is in a bucket, anything else is local. Every command
takes `--context NAME` or `UC_KUBE_CONTEXT`.

```bash
uc fs ls                                 # the buckets
uc fs ls backups:2026/                   # one level under a prefix
uc fs ls -r backups:                     # everything
uc fs mb backups                         # make a bucket
uc fs cp db.tar.gz backups:              # backups:db.tar.gz
uc fs cp a.txt b.txt backups:2026/       # backups:2026/a.txt and b.txt
uc fs cp notes.md backups:today.md       # under another name
uc fs cp -r scripts backups:             # backups:scripts/...
uc fs cp backups:db.tar.gz .             # download into ./db.tar.gz
uc fs cp -r backups:scripts .            # into ./scripts/
uc fs cat backups:today.md               # print an object
uc fs rm backups:2026/a.txt
uc fs rm -r backups:2026/                # everything under a prefix
uc fs rb backups                         # remove a bucket once it's empty
```

`<bucket>:` or `<bucket>:<prefix>/` as the destination keeps the names;
`<bucket>:<key>` renames.

Another S3 than the cluster's:

```bash
UC_FS_ENDPOINT=https://s3.example.com UC_FS_ACCESS_KEY=... UC_FS_SECRET_KEY=... uc fs ls
```

`rm`, `rm -r` and `rb` delete for good: `ask: true`.
