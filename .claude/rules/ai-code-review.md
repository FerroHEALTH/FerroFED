<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Machine review (SonarQube Cloud): advisory, never authority

Every pull request and every push to `main` is analyzed by SonarQube Cloud
(`.github/workflows/sonar.yml`; scope in `sonar-project.properties`; project
`FerroHEALTH_FerroFED`, organization `ferrohealth`, the built-in "Sonar
way" quality gate). It exists as a second opinion beside the local gates and
CodeQL, and it also reads the trees the Rust gates never see: shell, workflow
YAML, and JSON.

The lane runs the multi-language sweep and the Rust analysis (the analyzer
runs Clippy itself over the workspace), and imports the lcov report the
coverage step writes one workspace member at a time (`ci-cd.md`).

It is a **second opinion**. It is not authority, and it gates no merge.

After each analysis of `main`, the workflow converts the open issues to SARIF
(`scripts/sonar/sarif.sh`) and uploads them to GitHub code scanning under the
category `sonarqube-cloud`, so SonarQube Cloud is listed as a code-scanning
tool beside CodeQL and Scorecard. Sonar's own code-scanning integration needs
its Enterprise plan. Pull requests are not uploaded: a pull-request analysis
lists only the issues the change adds, and code scanning would read every
other alert as fixed. SonarQube Cloud decorates the pull request itself.
Those alerts are as advisory as the dashboard.

## Precedence: a finding never outranks the sources

1. The federation specification, openEHR ITS-REST, AQL and the RM, and the
   IHE and Dutch bindings
   (`spec-adherence.md`): the oracle.
2. The hard rules: `CLAUDE.md` and the `.claude/rules/*.md` files.
3. The local gates: `shellcheck`, `actionlint`, `zizmor`, `cargo fmt`,
   `clippy`, `cargo nextest`, `cargo deny`, plus the CI guards.
4. The analyzer.

A finding that contradicts a spec citation, or asks for something the rules
forbid, is wrong by construction. Nothing it reports relaxes `testing.md`:
never weaken a test because a finding suggested it.

## Rules

- **It gates no merge.** The quality gate is informational; merges are decided
  by the local gates and review.
- **Findings are acted on by hand, in a normal change**, never applied through
  a UI that would attribute a commit to a bot. The no-AI-attribution rule has
  no exceptions.
- **Automatic Analysis stays OFF.** CI-based analysis and SonarQube Cloud's
  Automatic Analysis cannot both run; `sonar.yml` is the analysis path.
- **A wrong finding is data, not a silent suppression:** record it on the
  tracker, or adjust `sonar-project.properties` when the scope is genuinely
  wrong, never to make a number go down.

CodeQL (`.github/workflows/codeql.yml`) runs separately as the security
scanner and is advisory here too, until a precision case is made to gate on it.

**The integration-test trees are out of CodeQL's Rust scope** (#302): the
`paths-ignore` entry `**/tests/**` in `.github/codeql/codeql-config.yml`
leaves them out, because their sinks are assertion and panic messages over
synthetic fixtures, which `rust/cleartext-logging` reads as logging. The
logging surfaces that rule should cover (the request log, the panic hook, the
tracing output, the startup banner) live under `src/` and stay in scope. A
`#[cfg(test)]` module under `src/` stays in scope too, and an alert there is
dismissed one at a time with a reason that names the issue, never quieted by
changing the test. A finding in production code is fixed in the code: the
startup banner takes only the values it prints, so no struct carrying a
credential reaches stdout.

## Official documentation (durable citations)

- <https://docs.sonarsource.com/sonarqube-cloud/>
- <https://docs.sonarsource.com/sonarqube-cloud/analyzing-source-code/languages/rust>
