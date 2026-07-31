# Concepts

Shared domain vocabulary for this project -- entities, named processes, and status
concepts with project-specific meaning. Seeded with core domain vocabulary, then
accretes as ce-compound and ce-compound-refresh process learnings; direct edits
are fine. Glossary only, not a spec or catch-all.

## Relationships

A **survey** reads a database and produces an **artifact**. An artifact holds a
**schema document** (or a **server document** when a whole server was surveyed),
and optionally **samples** drawn from the tables. A **report** is rendered from an
artifact and never from a database.

The split is deliberate: producing an artifact requires credentials and network
access to the database; rendering a report requires neither, so an artifact can
be carried out of a restricted environment and read anywhere.

## Survey

One complete pass over a database: connect, read the structure, optionally read a
bounded sample of rows, write an artifact. A survey is read-only by definition --
an implementation that issued a write would be a defect, not a configuration
choice.

A survey covers one database. Surveying every database on a server produces a
server document instead, with one schema document per database reached.

## Artifact

The portable file a survey writes. An artifact is self-describing and travels
independently of the database it came from -- it is the unit an operator carries
out of a restricted environment.

An artifact may be plain, compressed, encrypted, or compressed-and-encrypted. The
framing is decided when it is written and recovered when it is read; a reader
does not have to be told which it is holding.

## Schema document

The structured description of one database: its tables, columns, keys, indexes,
constraints, views, routines, triggers, and user-defined types, plus metadata
about how and when it was collected. This is what an artifact carries.

A schema document never contains a credential. That is enforced rather than
assumed: every path that writes or reads one scans it, and a document carrying a
credential is neither writable nor loadable.

## Server document

The multi-database counterpart of a schema document: facts about a server plus
one schema document per database surveyed. It carries its own discriminator, so a
reader can tell the two kinds apart without inspecting the contents.

A database that could not be read appears with a failure status rather than being
omitted -- the absence of a database and the failure to read one are different
facts.

## Sample

Rows read from a table and stored in an artifact. Samples are the only part of a
survey that touches user data, which is why they are opt-in rather than default.

A sample records not just the rows but how they were chosen -- the ordering
strategy used, the limit requested, and whether the read completed, was retried
at a lower limit, or was skipped. That metadata is what lets a reader tell an
empty table from one nobody could read.

## Redaction mode

How aggressively sampled values are masked before they reach a file. The modes
form a ladder, each masking a superset of the one below it: none, minimal,
balanced, conservative.

Redaction is idempotent -- applying it at collection time and again at report time
cannot un-redact anything, and cannot mask an already-masked value into something
else.

## Adapter

The per-engine implementation that knows how to read one database's catalog.
Every adapter satisfies the same contract, so orchestration never branches on
which engine it is talking to.

Adapters are selected by explicit wiring rather than by registration, so the set
of engines a binary can reach is visible at one place instead of being the side
effect of an import.

## Report

A human-readable rendering of an artifact. A report is derived, never
authoritative -- it is regenerated from the artifact rather than edited.

Rendering reads a file and nothing else: no network call, no connection string.

## Flagged ambiguities

- **Schema** is overloaded. It means the document this tool produces, and it also
  means a namespace inside a database (PostgreSQL and SQL Server have them;
  SQLite and MongoDB do not). Prefer "schema document" for the former and
  "namespace" or the engine's own word for the latter when both are in play.
