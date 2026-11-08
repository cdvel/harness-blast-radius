# GitHub Action

The action audits the repository's Claude Code configuration in CI and fails the job when risky settings remain after the baseline.

```yaml
name: harness-blast
on: [push, pull_request]
jobs:
  audit:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      security-events: write # only with upload-sarif
    steps:
      - uses: actions/checkout@v7
        with:
          persist-credentials: false
      - uses: cdvel/harness-blast-radius@v0.2.0 # pin to the release commit SHA
        with:
          baseline: .harness-blast-baseline.json
          upload-sarif: "true"
```

## Inputs

| Input | Default | Meaning |
|---|---|---|
| `path` | `.` | Project directory to audit |
| `persona` | `regular` | `regular`, `pedantic` or `auditor` |
| `baseline` | none | Baseline file of accepted findings, see [rules.md](rules.md#baseline) |
| `sarif-file` | `harness-blast.sarif` | Where the SARIF report is written |
| `upload-sarif` | `false` | Upload the report to GitHub code scanning; the job needs `security-events: write` |
| `fail-on-findings` | `true` | Fail when findings remain after the baseline |
| `install` | `true` | Download the release binary; `false` uses `harness-blast` from `PATH` |
| `version` | the action's release | Release to install |
| `token` | `github.token` | Token for downloading the release and its attestation |

Outputs: `exit-code` (0, or 11, 12 or 13 for the highest finding) and `sarif-file`.

## Behavior

- **Verified binary.** The action downloads the release archive for the runner (Linux or macOS, x64 or ARM64) and runs `gh attestation verify` on it, requiring that it was built by this repository's release workflow from the tag `v<version>`. It does not run an archive that fails the check.
- **Repository only.** The audit runs with an empty home and managed policy directory, so findings are about what the repository adds, not about the runner's own files.
- **Order.** The text report goes to the job log, then the SARIF report is written and uploaded, then the step fails if findings remain, so the upload happens either way.
- **Paths.** SARIF paths are relative to the repository root, so code scanning places alerts on the right line when `path` is a subdirectory.

The baseline is a file in the repository, passed by the workflow; changes to it show in the pull request like any other file.
