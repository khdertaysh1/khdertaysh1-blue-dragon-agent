# Collector completeness and namespace follow-up

Local continuation from 6419612. No remote upload, merge or release.

## Producer-to-review truncation correction

Independent review reproduced a gap in the preceding fixture: the shared page.js
collector clipped a 61-field form to 60 fields before review, hiding a token-like
name in field 61. The prior engine test supplied 61 directly, so it did not test
the real producer boundary. That test was insufficient for the shipped path.

The collector now emits forms_total/forms_omitted/forms_truncated and per-form
fields_total/fields_omitted/fields_truncated. Names also carry name_truncated.
Collection of forms and fields constructs only the bounded prefix, instead of
spreading the complete collections before slicing. It still returns at most
20 forms and 60 fields each; it does not submit anything or export field values.

The engine propagates omission counts and completeness. Legacy snapshots at
exactly 20 forms or 60 fields without reliable counters are incomplete. Missing,
inconsistent or truncated field metadata cannot yield a complete negative token
assessment. Truncated names also make a no-match assessment inconclusive.
Observed token names remain indicators, never proof of protection.

scripts/test_browser_forms.mjs executes the actual shared page.js in a mocked
DOM with 21 forms, including 61 fields with csrf_token at position 61. It compares
its JSON output exactly to scripts/fixtures/browser-form-limits.json. The Rust
engine test consumes that same fixture and checks one omitted form, one omitted
field, incomplete metadata and an inconclusive token assessment. The marker and
private field value are absent from the collected output. This verifies the
producer-to-consumer data contract; it is not a graphical browser test.

To regenerate deliberately after reviewing a collector change:

    node scripts/test_browser_forms.mjs --write-fixture

Ordinary validation does not regenerate it:

    node scripts/test_browser_forms.mjs

## Namespace imports

A query helpers.decode can now suggest a local trace for decode when the source
has import * as helpers from a cached resolvable URL. Bare packages, default
bindings and arbitrary nested members are not guessed. Tests cover successful
namespace follow-up and reject helpers.decode.nested, helpers alone and
helpersExtra.decode. Existing named aliases and selected-source tracing remain
covered. Each follow-up explicitly has semantic_link_verified false: lexical
shadowing, runtime import maps, types, dispatch and full taint are unresolved.

## Validation

190 Rust tests passed, along with the JS collector/form/readiness contracts,
Clippy all targets with warnings denied, formatting/diff checks and release build.
Nine engine CLI scenarios and the continuity smoke passed on the final binary;
results are in validation/collector-trace-smoke.jsonl. The expanded module fixture
required selecting the named alias explicitly because it now contains two imports
from the same file; the corrected full suite was rerun successfully.

No physical Termux, native Windows/macOS, graphical browser or live model/vision
provider was tested for this batch. These are measured local corrections and
syntactic follow-ups, not complete browser coverage, semantic flow or Alpha parity.
