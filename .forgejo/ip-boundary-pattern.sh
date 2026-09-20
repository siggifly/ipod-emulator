# The proprietary-marker pattern, in one place so the self-test and the scan can
# never drift apart. Sourced by `.forgejo/workflows/ip-boundary.yml`.
#
# Each alternative and why it is safe against this repository's legitimate text:
#
#   ADR-[0-9]{3}   an ADR citation. Bare "ADR" is not matched — it needs digits.
#   Veldi          the brand, capital V. The forge hostname is lowercase.
#   veldi-io       the org, hyphenated. The hostname is `veldi.io`, dotted.
#   atilo          the MSP brand. Always lowercase, and has no business here.
#   arfi-veldi     the MCP server that must never be called from this tree.
#   Arfi-Co        the holdco org.
#   standards/adr  the proprietary standards tree.
export IP_PATTERN='ADR-[0-9]{3}|Veldi|veldi-io|atilo|arfi-veldi|Arfi-Co|standards/adr'
