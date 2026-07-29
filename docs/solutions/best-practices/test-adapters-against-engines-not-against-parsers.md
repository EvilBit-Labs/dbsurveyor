---
title: Test database adapters against engines, not against your own parser
date: 2026-07-28
category: best-practices
module: adapters
problem_type: best_practice
component: testing_framework
severity: high
applies_when:
  - Writing an adapter over a database catalog or any external system
  - A unit test asserts behavior by feeding a function strings you wrote yourself
  - Container-backed tests exist but have never actually run
tags: [testing, integration-tests, testcontainers, adapters, postgres, mongodb]
---

# Test database adapters against engines, not against your own parser

## Context

Six database adapters were written with unit tests passing and
`golangci-lint` clean. The container-backed integration suites existed, were
linted and type-checked, but had never executed -- no container runtime was
available on the development machine.

Their first run in CI found four defects. Three could not have been caught by any
unit test, because the unit tests were asserting against inputs that the engines
never actually produce.

## Guidance

**When a unit test constructs the input, it tests your model of the system, not
the system.** That is fine for pure logic and actively misleading for anything
that parses, scans, or maps what an external system returned.

The clearest case here: PostgreSQL index sort direction.

```go
// The unit test fed the parser a string the author believed pg_get_indexdef
// returns, and asserted the parser handled it. It passed for weeks.
assert.Equal(t, descending, indexColumn("created_at DESC").SortOrder)
```

`pg_get_indexdef(..., pretty := true)` **omits the ordering options entirely**.
The engine never emits `created_at DESC` in that form, so every descending index
was reported as ascending in production while the test stayed green.

The fix was not a better parser. It was to stop parsing and read the value from
where PostgreSQL actually records it:

```sql
-- indoption bit 0 is the DESC flag; this is authoritative
ARRAY(SELECT (ix.indoption[k - 1] & 1) = 1
      FROM generate_series(1, ix.indnkeyatts) AS k)
```

**Prefer the structured field over the rendered text.** A catalog that offers
both a human-readable rendering and a machine field is telling you which one it
promises the shape of.

## Why This Matters

The four defects, and what each one says:

| Defect | Why a unit test missed it |
| ------ | ------------------------- |
| `describe` failed on every table: `cannot scan NULL into *bool` | `false OR NULL` is `NULL` in SQL, not `false`. Only a real engine evaluates SQL three-valued logic. |
| Every descending index reported ascending | The test asserted against a string the engine does not emit. |
| MongoDB inferred no nested fields | A subdocument does not always decode as `bson.M`; the driver may hand back `bson.D`. Inference stopped at the first level and *looked correct* because the parent field was still reported. |
| MySQL `BLOB` length mismatch | This one was a wrong test, not wrong code -- `BLOB` genuinely has a 65,535-byte ceiling. |

Note the third one especially: the failure was silent and plausible. The output
contained `address` but not `address.city`, which reads as a shallow document
rather than as broken inference.

The fourth is the counter-case worth keeping honest about: when an integration
test fails, the implementation is not automatically the thing that is wrong.

## When to Apply

- Any adapter over a database catalog, an API, or a wire protocol
- Any code that scans results whose nullability or type depends on the query
- Before claiming an adapter works, when its container tests have never run

State plainly when container-backed tests have not executed. "Tests pass" that
silently excludes the suites which exercise the actual subject is a claim that
will be read as stronger than it is.

## Examples

Guard SQL boolean expressions that can go NULL, rather than discovering it at
scan time:

```sql
-- Wrong: pg_get_expr is NULL when a column has no default, so the whole
-- disjunction is NULL and the scan into a bool fails.
a.attidentity <> '' OR pg_get_expr(d.adbin, d.adrelid) LIKE 'nextval(%'

-- Right
COALESCE(a.attidentity <> '' OR pg_get_expr(d.adbin, d.adrelid) LIKE 'nextval(%', false)
```

Accept every shape a driver may hand back, rather than the one it happened to
return in a fixture:

```go
func asDocument(value any) (bson.M, bool) {
	switch typed := value.(type) {
	case bson.M:
		return typed, true
	case map[string]any:
		return typed, true
	case bson.D:
		flattened := make(bson.M, len(typed))
		for _, element := range typed {
			flattened[element.Key] = element.Value
		}

		return flattened, true
	default:
		return nil, false
	}
}
```

## Related

- Integration suites sit behind an `integration` build tag so the ordinary
  `go test ./...` needs no container runtime -- but they are still linted and
  type-checked, because an untagged file is a file nothing compiles.
- `GOTCHAS.md` section 1 records two repository-level tests in this tree that
  reported success over an empty set for months. Same lesson, different shape:
  a test that has never failed is not evidence.
