# Message Reference Artifact Fixtures

`v0.1/canonical.json` and `v0.1/zero-references.json` contain exact canonical writer payloads followed by one repository text-file LF. The conformance test removes that fixture framing byte and requires the payload itself to contain no insignificant whitespace or final newline.

`v0.1/noncanonical-input.json` is an accepted decoder input. Its member order, whitespace, and equivalent string escape are intentionally noncanonical. It must decode to the same typed value as `v0.1/zero-references.json` and re-encode as that canonical fixture.

The JSON Schema fixes the portable v0.1 JSON shape. `intlify_contract` remains normative for UTF-8 byte limits, domain-specific selector grammar, duplicate member rejection, integer lexical spelling, cross-field invariants, error precedence, and canonical emission.
