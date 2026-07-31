package envelope

import (
	"errors"
	"fmt"

	"golang.org/x/crypto/argon2"
)

// The Argon2id cost parameters this implementation writes.
//
// The values follow RFC 9106 and the OWASP Password Storage Cheat Sheet for a
// high-security, non-interactive workload: 64 MiB of memory makes GPU and ASIC
// attacks expensive, three passes over that memory is the RFC's recommendation
// at this memory cost, and four lanes uses a modern CPU without monopolizing
// it. A derivation costs tens to a few hundred milliseconds depending on
// hardware, which is the point -- a password guess costs the attacker the same.
const (
	// DefaultMemoryKiB is the Argon2id memory cost in KiB (64 MiB).
	DefaultMemoryKiB uint32 = 65536
	// DefaultTime is the number of Argon2id passes over the memory.
	DefaultTime uint32 = 3
	// DefaultParallelism is the number of Argon2id lanes.
	DefaultParallelism uint8 = 4
)

// Sizes fixed by the format.
const (
	// SaltLength is the Argon2id salt size in bytes. RFC 9106 section 4 sets
	// 128 bits as the minimum for uniqueness across all derivations.
	SaltLength = 16
	// KeyLength is the derived key size in bytes, 256 bits for AES-256.
	KeyLength = 32
)

// The cost ceilings enforced when reading an envelope.
//
// KDF parameters arrive from the file, and the key must be derived before the
// authentication tag can be checked, so these numbers are consumed while still
// unauthenticated. Without a ceiling a hostile file could name a memory cost of
// several terabytes and turn opening it into an out-of-memory crash. The
// ceilings are far above anything this implementation writes; they bound the
// damage rather than express a preference.
const (
	maxMemoryKiB   uint32 = 1 << 20 // 1 GiB
	maxTime        uint32 = 16
	maxParallelism uint8  = 16
)

// ErrWeakKDFParameters reports KDF parameters below the security floor.
var ErrWeakKDFParameters = errors.New("kdf parameters are below the security floor")

// ErrExcessiveKDFParameters reports KDF parameters above the cost ceiling.
var ErrExcessiveKDFParameters = errors.New("kdf parameters exceed the cost ceiling")

// KDFParams are the Argon2id inputs recorded in an envelope header.
//
// They are stored rather than assumed so that raising the cost later does not
// make existing envelopes unreadable. The floor moves the other way: lowering
// it, or accepting a value beneath it, would silently weaken every envelope
// written since, so a value below the floor is refused and a lower floor would
// be a format version change.
type KDFParams struct {
	// MemoryKiB is the memory cost in KiB.
	MemoryKiB uint32
	// Time is the number of passes over the memory.
	Time uint32
	// Parallelism is the number of lanes.
	Parallelism uint8
	// Salt is the per-envelope random salt, SaltLength bytes.
	Salt []byte
}

// DefaultKDFParams returns the parameters this implementation writes, carrying
// the supplied salt.
func DefaultKDFParams(salt []byte) KDFParams {
	return KDFParams{
		MemoryKiB:   DefaultMemoryKiB,
		Time:        DefaultTime,
		Parallelism: DefaultParallelism,
		Salt:        salt,
	}
}

// Validate reports whether the parameters are usable: a correctly sized salt,
// costs at or above the security floor, and costs at or below the ceiling that
// bounds an unauthenticated allocation.
func (p KDFParams) Validate() error {
	if len(p.Salt) != SaltLength {
		return fmt.Errorf("%w: salt is %d bytes, expected %d", ErrMalformedHeader, len(p.Salt), SaltLength)
	}

	switch {
	case p.MemoryKiB < DefaultMemoryKiB:
		return fmt.Errorf("%w: memory cost %d KiB is below %d KiB", ErrWeakKDFParameters, p.MemoryKiB, DefaultMemoryKiB)
	case p.Time < DefaultTime:
		return fmt.Errorf("%w: time cost %d is below %d", ErrWeakKDFParameters, p.Time, DefaultTime)
	case p.Parallelism < 1:
		return fmt.Errorf("%w: parallelism must be at least 1", ErrWeakKDFParameters)
	}

	switch {
	case p.MemoryKiB > maxMemoryKiB:
		return fmt.Errorf(
			"%w: memory cost %d KiB is above %d KiB",
			ErrExcessiveKDFParameters,
			p.MemoryKiB,
			maxMemoryKiB,
		)
	case p.Time > maxTime:
		return fmt.Errorf("%w: time cost %d is above %d", ErrExcessiveKDFParameters, p.Time, maxTime)
	case p.Parallelism > maxParallelism:
		return fmt.Errorf("%w: parallelism %d is above %d", ErrExcessiveKDFParameters, p.Parallelism, maxParallelism)
	}

	return nil
}

// derive runs Argon2id over the password and returns a KeyLength key.
//
// The caller owns the returned key and should Zero it once the cipher has been
// constructed. Derivation allocates MemoryKiB and takes long enough to be
// noticeable, so a caller must not hold a lock across it.
func (p KDFParams) derive(password []byte) []byte {
	return argon2.IDKey(password, p.Salt, p.Time, p.MemoryKiB, p.Parallelism, KeyLength)
}
