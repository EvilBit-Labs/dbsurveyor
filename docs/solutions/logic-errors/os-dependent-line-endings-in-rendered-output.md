---
title: Rendered output inherited the host's line endings
date: 2026-07-28
category: logic-errors
module: report
problem_type: logic_error
component: documentation
symptoms:
  - Golden-file tests pass on Linux and macOS and fail on Windows CI
  - A test diff shows identical-looking lines marked as different
  - Two renders of the same artifact differ byte for byte across platforms
root_cause: wrong_api
resolution_type: code_fix
severity: high
tags: [line-endings, crlf, golden-files, portability, windows, gitattributes]
---

# Rendered output inherited the host's line endings

## Problem

The Markdown report renderer emitted CRLF on Windows and LF everywhere else, so
a report depended on the operating system that produced it. Two reports of the
same input artifact differed byte for byte, and a checksum over a report meant
nothing.

## Symptoms

- `Test (windows-latest)` failed while Linux and macOS passed
- The failure diff showed table separator rows that looked character-identical:

  ```
  +|---------|---------|
  -|---------|---------|
  ```

- Golden files could only ever pass on the platform that generated them

## What Didn't Work

- Reading the diff. The differing bytes are invisible in a terminal, and the
  first instinct is to look for a whitespace or column-width bug that is not
  there. `od -c` is what actually shows it.
- Treating it as a golden-file problem. Regenerating the goldens would have made
  CI green on one platform and moved the failure to another, leaving the real
  defect -- platform-dependent artifacts -- in place.

## Solution

Normalize at the render boundary, so nothing downstream has to care:

```go
// normalizeLineEndings forces LF, whatever the host uses.
func normalizeLineEndings(rendered string) string {
	return strings.ReplaceAll(rendered, "\r\n", "\n")
}
```

Then keep git from reintroducing the problem from the other direction, since a
Windows checkout will otherwise convert the committed goldens to CRLF:

```gitattributes
# Golden files record byte-for-byte output, so they must not be rewritten by
# git's line-ending conversion on checkout.
*.golden -text
```

And assert both halves, so a regression is a test failure rather than a surprise
in somebody's report:

```go
assert.NotContains(t, rendered, "\r", "a report carries no carriage returns")

// and separately, that the checked-out fixture has none either
contents, err := os.ReadFile(filepath.Join("testdata", name))
require.NoError(t, err)
assert.NotContains(t, string(contents), "\r")
```

## Why This Works

The underlying library chose the line ending from `runtime.GOOS`:

```go
func LineFeed() string {
	if runtime.GOOS == "windows" {
		return "\r\n"
	}
	return "\n"
}
```

That is a reasonable default for a text editor and wrong for a tool whose output
is an artifact. Normalizing at the boundary makes the choice the program's rather
than the host's.

LF specifically, because every consumer of the output -- git, a Markdown
renderer, a terminal -- reads LF, and only some of them read CRLF.

The second test matters as much as the first. Without `.gitattributes`, the
committed goldens are LF in the repository and CRLF in a Windows working tree,
so the comparison fails again with the code entirely correct.

## Prevention

- **Decide line endings at the boundary where output is produced**, not per
  platform. Any library that branches on `runtime.GOOS` for formatting is a
  candidate to normalize after.
- **Run the test matrix on every platform you claim to support.** This was
  caught by `windows-latest` in CI and by nothing else; local development was
  entirely on Windows, where it passed.
- **Treat a golden-file failure that shows visually identical lines as a
  byte-level difference** and reach for `od -c` immediately.
- **`.gitattributes` is part of the fix, not a nicety** whenever a checked-in
  fixture asserts exact bytes.

## Related Issues

- The same reasoning applies to any artifact this tool writes. Schema documents
  are JSON written through `encoding/json`, which does not vary by platform, so
  only the Markdown path needed this.
