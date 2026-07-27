# Encrypted envelope format

The encrypted envelope is the binary container `dbsurveyor-collect` writes when
`--encrypt` is given, and `dbsurveyor` reads back. It wraps an arbitrary payload
-- in practice a schema document, compressed or not -- in AES-256-GCM under a
key derived from a password by Argon2id.

The Go implementation in `internal/envelope` is the reference for this format.
It carries no obligation to read envelopes produced by the retired Rust
implementation, and no Rust-era envelope is a valid envelope under this
specification.

This document and the implementation are held together by
`TestWorkedExampleMatchesTheSpecification`, which reproduces the worked example
below byte for byte from injected salt and nonce values. A change to the layout
that is not also a change to this document is a test failure.

## Overview

```text
+--------------------------------+-------------------+------------+
| header (50 bytes, plaintext)   | ciphertext (n)    | tag (16)   |
+--------------------------------+-------------------+------------+
 \______________________________/ \_____________________________/
   additional authenticated data     AES-256-GCM output
```

The header is not encrypted -- a reader needs it to derive the key -- but it is
**authenticated**: the whole 50-byte header is passed to GCM as additional
authenticated data. Altering a single bit of it fails the tag check exactly as
altering the ciphertext does.

## Byte layout

All multi-byte integers are big-endian.

| Offset | Size | Field | Value |
| --- | --- | --- | --- |
| 0 | 8 | Magic | ASCII `DBSVENC1` |
| 8 | 1 | Format version | `0x01` |
| 9 | 1 | KDF identifier | `0x01` = Argon2id |
| 10 | 1 | Cipher identifier | `0x01` = AES-256-GCM |
| 11 | 1 | Argon2id parallelism | lanes, `0x04` as written |
| 12 | 4 | Argon2id memory cost | KiB, `0x00010000` (65536) as written |
| 16 | 4 | Argon2id time cost | passes, `0x00000003` as written |
| 20 | 1 | Salt length | `0x10` (16) |
| 21 | 1 | Nonce length | `0x0C` (12) |
| 22 | 16 | Salt | random, per envelope |
| 38 | 12 | Nonce | random, per envelope |
| 50 | n | Ciphertext | same length as the plaintext |
| 50 + n | 16 | Authentication tag | GCM tag, 128 bits |

Minimum envelope size is 66 bytes: a 50-byte header and a 16-byte tag over an
empty payload.

### Field notes

- **Magic.** The trailing `1` is not the format version. It distinguishes this
  magic from any future format that is not a version of this one, so that a
  wholly different container can be recognized instead of being reported as a
  corrupt envelope.
- **Algorithm identifiers** are single bytes rather than names. A
  length-prefixed string would be one more thing to parse before anything has
  been authenticated, on data that by definition came from an untrusted file.
- **Salt length and nonce length** are recorded even though both are fixed. They
  let a reader reject a mismatch as a malformed header rather than silently
  slicing at the wrong offset, and they name the assumption the fixed offsets
  above depend on. A header declaring any other value is refused before a key is
  derived.
- **Cost parameters** are recorded so that raising the cost later does not make
  existing envelopes unreadable. The floor moves the other way: see below.
- **No payload description.** The header says nothing about what it wraps. The
  payload's format, its size before encryption, and the file it came from are
  not recoverable from an envelope. Whether the decrypted payload is compressed
  is determined from the payload itself -- see `compression.md`.

## Key derivation

Argon2id (RFC 9106), version 0x13, with a 32-byte output for AES-256.

| Parameter | Value written | Rationale |
| --- | --- | --- |
| Memory | 65536 KiB (64 MiB) | OWASP recommends 19 MiB minimum for interactive logins; 64 MiB is the high-security tier and makes GPU and ASIC attacks expensive |
| Time | 3 passes | RFC 9106 section 4 at this memory cost |
| Parallelism | 4 lanes | Uses a modern CPU without monopolizing it |
| Salt | 16 bytes, random per envelope | RFC 9106 section 4 sets 128 bits as the minimum for uniqueness |

A derivation costs tens to a few hundred milliseconds depending on hardware.
That cost is the security property, not a defect: a password guess costs an
attacker the same. A caller must not hold a lock across it.

### Floor and ceiling

The recorded parameters are checked against both bounds before a key is derived.

- **Floor.** A memory cost below 65536 KiB, a time cost below 3, or a
  parallelism below 1 is refused. Accepting an envelope written with weaker
  parameters would silently accept weaker protection. The floor may only rise
  with a format version bump, which is what the version byte is for.
- **Ceiling.** A memory cost above 1 GiB, a time cost above 16, or a parallelism
  above 16 is refused. This bound exists because the parameters are consumed
  while still unauthenticated: the key must be derived before the tag can be
  checked, so a hostile file could otherwise name a memory cost of several
  terabytes and turn opening it into an out-of-memory crash. The ceiling is far
  above anything this implementation writes; it bounds the damage rather than
  expressing a preference.

## Password sourcing

Both binaries resolve the password the same way:

1. `DBSURVEYOR_ENCRYPTION_PASSWORD`, when the variable is set. A variable that
   is set but empty counts as set and fails as too short -- an operator who
   exported it meant to use it, and falling through to a prompt would hide the
   mistake.
2. Otherwise an interactive prompt, written to stderr with echo disabled, so
   that piping stdout to a file or another process never captures it. When
   sealing, the prompt is repeated for confirmation, because a typo would
   otherwise produce a file nobody can open. A password from the environment is
   not confirmed: reading the same variable twice cannot disagree with itself.
3. With no terminal attached and no variable set, the run fails and names the
   variable rather than blocking.

A password shorter than 8 characters is refused, identically from either source.
Eight characters is a floor against typos and empty variables, not a claim that
an eight-character password is strong -- what makes a guess expensive is the
Argon2id cost.

**Erasure is best effort and no more.** Passwords and derived keys are held as
`[]byte` and overwritten after use, but a password read from the environment
arrives as a Go string that neither it nor the process environment block can
overwrite, and the garbage collector may have copied any of these bytes while
moving them, with no way to find those copies. This tree makes no claim of
deterministic zeroization.

## Failure behavior

- A wrong password and an altered file produce **the same error**. They are the
  same event to the cipher, and reporting them apart would tell whoever altered
  the file whether their password guess was right.
- Opening returns no plaintext on any failure, not a truncated or partial
  payload.
- Header checks -- magic, version, algorithm identifiers, declared field lengths,
  cost bounds -- run before a key is derived, so a malformed file is rejected
  without paying for a derivation.

## Worked example

This vector is deterministic. The salt and nonce are drawn in that order from a
byte stream counting up from zero, so the salt is `00 01 ... 0f` and the nonce is
`10 11 ... 1b`.

| Input | Value |
| --- | --- |
| Password | `example-password` |
| Plaintext | `{"format":"dbsurveyor/schema"}` (30 bytes) |
| Salt | `000102030405060708090a0b0c0d0e0f` |
| Nonce | `101112131415161718191a1b` |

Derived key (Argon2id with the parameters above):

```text
2f66cd3aefa45a0cf591eb28b6d1dd3bfc0113116b95f5d8ad3ab1a0d4aa55ec
```

The key is published here so that an implementer can tell a mis-wired Argon2id
apart from a mis-wired GCM. It is not a secret: the password it comes from is
printed directly above it.

Complete envelope, 96 bytes:

```text
44425356 454e4331 01 01 01 04 00010000 00000003 10 0c
000102030405060708090a0b0c0d0e0f
101112131415161718191a1b
b5404a04fed77a2e2f84a24f377847a4d9f98f848a2b6bfe76cf9f6f08e6
032b9c081bd8f9cc10aea109f14a7b6b
```

Read line by line: the header fields through the nonce length, the salt, the
nonce, the 30-byte ciphertext, and the 16-byte tag.

As one string:

```text
44425356454e4331010101040001000000000003100c000102030405060708090a0b0c0d0e0f
101112131415161718191a1bb5404a04fed77a2e2f84a24f377847a4d9f98f848a2b6bfe76cf
9f6f08e6032b9c081bd8f9cc10aea109f14a7b6b
```

(Wrapped for width; the envelope is a single 96-byte sequence with no
separators.)

## What an envelope never contains

- The password, the derived key, or anything from which either can be recovered
  more cheaply than by guessing the password.
- A checksum or length of the plaintext. Both would leak information about the
  payload to anyone holding the file, and the GCM tag already covers integrity.
- Any identifier of the collecting host, the source database, or the operator.

## Related specifications

- `schema-document.md` -- the JSON document that is usually the payload
- `compression.md` -- zstd framing and output extension dispatch
