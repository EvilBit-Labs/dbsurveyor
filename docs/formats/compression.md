# Artifact framing and extension dispatch

An *artifact* is a schema document as it exists on disk. Four framings are
possible, chosen by the `--compress` and `--encrypt` flags, and this document
specifies how each is named, how it is written, and how it is read back.

The Go implementation in `internal/artifact` is the reference. It carries no
obligation to read artifacts produced by the retired Rust implementation.

Framing is where a mistake is quiet rather than loud: a reader that trusts the
wrong signal produces a confusing error at best and reads the wrong bytes at
worst. The rules below are stated precisely for that reason.

## The four framings

| `--compress` | `--encrypt` | Extension | Contents |
| --- | --- | --- | --- |
| no | no | `.json` | UTF-8 JSON |
| yes | no | `.json.zst` | one zstd frame wrapping the JSON |
| no | yes | `.enc` | an envelope wrapping the JSON |
| yes | yes | `.enc` | an envelope wrapping one zstd frame wrapping the JSON |

Compression runs **before** encryption, never the reverse. Ciphertext is
indistinguishable from random and does not compress, so encrypting first would
pay for the compression pass and save nothing.

The envelope is the outermost frame, so a compressed-and-encrypted artifact is
named `.enc`. Its name says nothing about the zstd frame inside; see
*Reading* below.

## Naming

The requested output path gains the framing's extension unless it already ends
with it, so an operator who spells the extension out does not get it twice.
Plain and compressed artifacts are named after what they hold -- JSON, possibly
zstd-framed -- so a bare name gains `.json` first.

| Requested | Framing | Written |
| --- | --- | --- |
| `schema` | plain | `schema.json` |
| `schema` | compressed | `schema.json.zst` |
| `schema` | encrypted | `schema.enc` |
| `schema` | both | `schema.enc` |
| `schema.json` | compressed | `schema.json.zst` |
| `schema.json.zst` | compressed | `schema.json.zst` |
| `schema.enc` | encrypted | `schema.enc` |

The comparison ignores case, because a path from a case-insensitive filesystem
may arrive as `SCHEMA.ENC` and appending `.enc` to that would give a file two
extensions for one framing.

The writer returns the path it actually created. Callers report that path rather
than the one they requested; the two differ whenever normalization applied.

## Writing

1. **Validate.** The document is validated first, which includes the recursive
   credential scan. A document carrying a credential fails here, before any byte
   reaches disk.
2. **Marshal** to JSON.
3. **Compress**, if requested, into a single zstd frame.
4. **Seal**, if requested, in an envelope (`encrypted-envelope.md`).
5. **Write atomically**, as described below.

### The atomic-write contract

Bytes go to a temporary file, are flushed, and are then renamed over the
destination.

- The temporary file is created **in the destination directory**, not in the
  system temporary directory. A rename within one filesystem is atomic; a rename
  across filesystems degrades to a copy, which is exactly the partially written
  destination this exists to prevent.
- Its name begins with a dot and ends in `.tmp`, so a partial write stays out of
  a casual listing and an abandoned one is recognizable.
- The contents are flushed before the rename. A crash therefore leaves the
  destination either absent or holding the previous complete artifact -- never a
  truncated file.
- A failure at any step removes the temporary file. The error reported is the
  reason the write failed, not the reason the cleanup failed.
- Artifacts are created mode `0600`. A schema document describes the shape of a
  database the operator has access to, which is reconnaissance material even
  with every value redacted.

Durability of the directory entry itself after the rename is left to the
filesystem. What is guaranteed here is that a reader never observes a truncated
artifact.

## Reading

Dispatch is on the **final extension only**, and then, inside an envelope, on
the zstd frame magic.

```text
path
 |
 +-- .enc  -> open envelope -> begins with 28 B5 2F FD ? -> yes: decode zstd
 |                                                       -> no:  use as-is
 +-- .zst  -> must begin with 28 B5 2F FD -> decode zstd
 |
 +-- else  -> use as-is
                          |
                          v
              recursive credential scan
                          |
                          v
                  decode and validate
```

Two asymmetries in that picture are deliberate.

**Inside an envelope, the bytes are asked, not the name.** A
compressed-and-encrypted artifact is named `.enc` and there is no room in that
name for the zstd frame within. The four-byte zstd frame magic (`28 B5 2F FD`,
RFC 8878 section 3.1.1) is checked on the decrypted payload instead. This is
safe to do on decrypted bytes because they have already passed the envelope's
authentication tag.

**A `.zst` artifact is not sniffed.** A file named `.zst` that does not begin
with a zstd frame is rejected, rather than being read as JSON. Falling back
would teach an operator that the extension carries no meaning, and the usual
cause of the mismatch -- a rename -- is worth reporting rather than papering
over.

### The decompression limit

A zstd frame reaches ratios in the thousands on repetitive input, so a small
hostile file can name an enormous decompressed size. On the plain `.zst` path
there is no authentication tag to check first, so the reader refuses anything
expanding past **1 GiB**. A schema document for a database large enough to
approach that is well past what this tool is built for.

### The credential scan is the last gate

Every read path terminates in the recursive credential scan, and the scan runs
on the **JSON bytes**, not on the decoded document.

That ordering matters. Unmarshalling silently drops any field the Go types do
not declare, so a scan that ran after decoding would pronounce a file clean
while a credential sat in a field this version does not know about. What the
operator holds is the bytes, so the bytes are what is scanned.

The decoded document is then validated, which runs its own scan. The overlap is
deliberate: validation is the enforcement point for documents that never touched
disk, and this package does not get to assume it is the only caller.

## Failure behavior

| Situation | Result |
| --- | --- |
| File absent or unreadable | the underlying error, naming the path |
| `.zst` that is not a zstd frame | format mismatch, naming the path |
| `.enc` that is not an envelope | format mismatch, naming the path |
| `.enc` with the wrong password or an altered byte | envelope authentication failure |
| Encrypted artifact with no password source | password required |
| Frame expanding past the limit | decompression limit exceeded |
| JSON that does not parse | decode error, naming the path |
| Document carrying a credential | one error per finding, naming the path and the rule |
| Document failing a format rule | every violation, joined |

Every failure names the file. An operator loading an artifact usually has
several on hand, and an error that says only "malformed" does not say which one
to look at.

A credential error names the path within the document and the rule that fired --
never the matched value. Reporting the value would copy a credential into an
error string, a log line, and eventually a terminal or a CI transcript, which is
the outcome the scan exists to prevent.

## Related specifications

- `schema-document.md` -- the JSON document that is framed here
- `encrypted-envelope.md` -- the AES-256-GCM container used by `.enc`
