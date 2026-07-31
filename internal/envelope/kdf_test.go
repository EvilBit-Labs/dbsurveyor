package envelope

import (
	"bytes"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func testSalt() []byte {
	return bytes.Repeat([]byte{0xA5}, SaltLength)
}

func TestDefaultParamsRecordTheDocumentedCosts(t *testing.T) {
	params := DefaultKDFParams(testSalt())

	assert.Equal(t, uint32(65536), params.MemoryKiB)
	assert.Equal(t, uint32(3), params.Time)
	assert.Equal(t, uint8(4), params.Parallelism)
	require.NoError(t, params.Validate())
}

func TestValidateRejectsAMissizedSalt(t *testing.T) {
	for name, salt := range map[string][]byte{
		"empty": nil,
		"short": bytes.Repeat([]byte{0x01}, SaltLength-1),
		"long":  bytes.Repeat([]byte{0x01}, SaltLength+1),
	} {
		t.Run(name, func(t *testing.T) {
			require.ErrorIs(t, DefaultKDFParams(salt).Validate(), ErrMalformedHeader)
		})
	}
}

func TestValidateRejectsCostsBelowTheFloor(t *testing.T) {
	cases := map[string]KDFParams{
		"memory":      {MemoryKiB: DefaultMemoryKiB - 1, Time: DefaultTime, Parallelism: DefaultParallelism},
		"time":        {MemoryKiB: DefaultMemoryKiB, Time: DefaultTime - 1, Parallelism: DefaultParallelism},
		"parallelism": {MemoryKiB: DefaultMemoryKiB, Time: DefaultTime, Parallelism: 0},
	}

	for name, params := range cases {
		t.Run(name, func(t *testing.T) {
			params.Salt = testSalt()
			require.ErrorIs(t, params.Validate(), ErrWeakKDFParameters)
		})
	}
}

func TestValidateRejectsCostsAboveTheCeiling(t *testing.T) {
	cases := map[string]KDFParams{
		"memory":      {MemoryKiB: maxMemoryKiB + 1, Time: DefaultTime, Parallelism: DefaultParallelism},
		"time":        {MemoryKiB: DefaultMemoryKiB, Time: maxTime + 1, Parallelism: DefaultParallelism},
		"parallelism": {MemoryKiB: DefaultMemoryKiB, Time: DefaultTime, Parallelism: maxParallelism + 1},
	}

	for name, params := range cases {
		t.Run(name, func(t *testing.T) {
			params.Salt = testSalt()
			require.ErrorIs(t, params.Validate(), ErrExcessiveKDFParameters)
		})
	}
}

// TestValidateAcceptsCostsAboveTheFloor is the compatibility half of the floor
// rule: raising the cost must stay readable, or an envelope written by a later
// build could not be opened by this one.
func TestValidateAcceptsCostsAboveTheFloor(t *testing.T) {
	params := KDFParams{
		MemoryKiB:   DefaultMemoryKiB * 2,
		Time:        DefaultTime + 1,
		Parallelism: DefaultParallelism + 1,
		Salt:        testSalt(),
	}

	require.NoError(t, params.Validate())
}

func TestDeriveProducesAKeyOfTheAESKeyLength(t *testing.T) {
	key := DefaultKDFParams(testSalt()).derive([]byte(testPassword))
	defer Zero(key)

	assert.Len(t, key, KeyLength)
	assert.NotEqual(t, make([]byte, KeyLength), key, "a derived key must not be all zeroes")
}

func TestDeriveIsDeterministic(t *testing.T) {
	first := DefaultKDFParams(testSalt()).derive([]byte(testPassword))
	defer Zero(first)

	second := DefaultKDFParams(testSalt()).derive([]byte(testPassword))
	defer Zero(second)

	assert.Equal(t, first, second)
}

func TestDeriveDependsOnTheSaltAndThePassword(t *testing.T) {
	base := DefaultKDFParams(testSalt()).derive([]byte(testPassword))
	defer Zero(base)

	otherSalt := bytes.Repeat([]byte{0x5A}, SaltLength)

	bySalt := DefaultKDFParams(otherSalt).derive([]byte(testPassword))
	defer Zero(bySalt)

	byPassword := DefaultKDFParams(testSalt()).derive([]byte("another-password"))
	defer Zero(byPassword)

	assert.NotEqual(t, base, bySalt, "a different salt must produce a different key")
	assert.NotEqual(t, base, byPassword, "a different password must produce a different key")
}

// TestDeriveDependsOnTheCostParameters is why the parameters are authenticated
// rather than merely recorded: they select the key.
func TestDeriveDependsOnTheCostParameters(t *testing.T) {
	base := DefaultKDFParams(testSalt())

	raised := base
	raised.Time = base.Time + 1

	baseKey := base.derive([]byte(testPassword))
	defer Zero(baseKey)

	raisedKey := raised.derive([]byte(testPassword))
	defer Zero(raisedKey)

	assert.NotEqual(t, baseKey, raisedKey)
}
