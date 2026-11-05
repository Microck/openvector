# Raft source CI

Source checks run in disposable Raft workspaces on existing, operator-owned hosts.
The controller runs them and posts a `raft/source` status to the exact GitHub commit.
There is no automatic push scheduler or registered GitHub self-hosted runner.
GitHub-hosted Actions are disabled while the account billing block remains.

## Check command

```sh
bash tools/check-source.sh
```

This installs the locked npm dependencies without lifecycle scripts, runs the
small-reference and independently authored smoke checks, checks JavaScript syntax,
and inspects the npm source package inventory without building native binaries.
It needs Node.js 22.19 or later and npm.
A green status covers these checks only. Native, full differential, and graphical
checks remain separate [release requirements](release-runbook.md).

## Run with stable Raft

The installed stable Raft 0.6.0 has no `raft run`. The
[disposable CI guide](https://raft.micr.dev/docs/guides/ci-jobs/) documents that
command as development-only. Use the stable lifecycle commands below. Do not
upgrade the shared CLI or copy host credentials into a guest just to run CI.

Use a trusted, clean checkout. Review its tracked files before export; a Git archive
is not a secret scanner. Select an existing location with enough capacity and follow
its owner/note policies. This source-only job needs one CPU and 2 GiB RAM.

```sh
raft --version
raft locations
raft capacity --location paris --json
raft list --location paris

commit=$(git rev-parse HEAD)
git archive --format=tar --output=/tmp/openvector-ci-source.tar "$commit"
box=$(raft new --location paris --ttl 900 --cpu 1 --memory 2GiB --disposable \
  --purpose "OpenVector source CI $commit" --owner "OpenVector CI" \
  --next "Collect logs and destroy")
```

Record the exact location-qualified handle immediately. An uncertain create or
transport failure is a reason to inspect that handle, not retry the job blindly.
Check that the guest can execute commands and has the required Node.js and npm:

```sh
raft exec "$box" -- node --version
raft exec "$box" -- npm --version
raft upload "$box" /tmp/openvector-ci-source.tar /workspace/source.tar
raft exec "$box" -- bash -lc 'mkdir /workspace/project && tar xf /workspace/source.tar -C /workspace/project'
raft exec "$box" -- bash -lc 'cd /workspace/project && timeout 600 bash tools/check-source.sh > /workspace/source-checks.log 2>&1'
```

Save the last command's actual exit status. Whether it succeeds or fails, collect
the log to a new local filename and destroy the exact owned workspace:

```sh
raft download "$box" /workspace/source-checks.log ./source-checks.log
raft destroy "$box"
raft info "$box"
raft list --location paris
```

The info command must report that the workspace is absent, and the list must exclude
it. A TTL is a backstop, not proof of cleanup. If collection or cleanup fails, report
that failure and inspect the exact handle. Do not mark CI successful yet.

## Report the result

After verifying execution and cleanup, post a commit status from the authenticated
controller. Keep tokens, SSH keys and host inventory outside the uploaded tree.
For a successful run:

```sh
gh api --method POST "repos/Microck/openvector/statuses/$commit" \
  -f state=success -f context=raft/source \
  -f description='Raft: source checks passed; disposable workspace removed'
```

Use `failure` for failing checks and `error` for an infrastructure, collection, or
cleanup failure. Post `pending` before starting when reporting a new run. Record the
commit, source archive hash, Raft version, guest tool versions, test count, command
exit code, log hash and cleanup result alongside the retained local log. Never reuse
a passing result for a different commit.

Raft workspaces share the host kernel. Only trusted project source may use this
controller and its host-level SSH access. This workflow does not run unreviewed
fork code or expose a host key to GitHub Actions.
