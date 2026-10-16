# Security policy

## Reporting a vulnerability

Report it privately. Do not open a public issue.

1. Use GitHub's private vulnerability reporting: the **Report a vulnerability** button on the repository's
   Security tab.
2. Or write to david@telosmachina.ai with "oaxaca-blinder-rs security" in the subject.

Give the version or commit, what you did, what happened, and what you expected. A proof of concept helps.

A fix for a confirmed vulnerability is released as a patch version of the latest 0.3 release, with a published
advisory.

## Supported versions

| Version | Supported |
|---|---|
| 0.3.x | yes |
| 0.2.x and older | no |

This is a 0.x project: only the latest 0.3 release receives fixes.

## In scope

- `oaxaca_blinder` (library and `oaxaca-cli`): a crash, memory-safety error or unbounded resource use on crafted
  input; a wrong result that a documented guarantee rules out.
- `meridian-mcp` over HTTP: the `MCP_API_KEY` check (header `x-api-key` or `Authorization`), the rate limit, the CORS
  policy, and anything that lets a caller without the key run an analysis.
- `pay-equity-engine`: the same classes as the library, and the JSON request and response handling.
- The release workflow and published artifacts.

## Out of scope

- Advisories against dependencies that are in `Cargo.lock` but not compiled into any shipped artifact. They are
  listed with a reason and an expiry date in `.cargo/audit.toml`; report one that is wrongly listed.
- Statistical choices that are documented (reference coefficients, weights, bootstrap defaults).
- Findings that need a modified build or a hostile host.
