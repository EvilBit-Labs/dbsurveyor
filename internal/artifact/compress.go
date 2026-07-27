package artifact

import (
	"bytes"
	"errors"
	"fmt"

	"github.com/klauspost/compress/zstd"
)

// zstdMagic is the zstd frame magic number, RFC 8878 section 3.1.1.
//
// It is what the read path uses to tell a compressed payload from a plain one
// after decryption, because a combined compressed-and-encrypted artifact is
// named ".enc" and its name says nothing about what is inside the envelope.
var zstdMagic = []byte{0x28, 0xB5, 0x2F, 0xFD}

// maxDecompressedSize caps what one artifact may expand to.
//
// zstd reaches ratios in the thousands on repetitive input, so a small hostile
// file can name an enormous decompressed size. Nothing in the payload is
// authenticated at this point on the plain ".zst" path -- there is no tag to
// check -- so the limit is the only thing standing between a crafted file and
// an out-of-memory crash. A schema document for a database large enough to
// approach a gigabyte of JSON is well past what this tool is built for.
const maxDecompressedSize = 1 << 30 // 1 GiB

// ErrTooLarge reports a compressed artifact that expands past
// maxDecompressedSize.
var ErrTooLarge = errors.New("compressed artifact exceeds the decompression limit")

// hasZstdMagic reports whether data begins with a zstd frame.
func hasZstdMagic(data []byte) bool {
	return bytes.HasPrefix(data, zstdMagic)
}

// compress frames data as a single zstd frame.
//
// The encoder is built per call rather than shared. A run writes one artifact,
// so there is nothing to amortize, and a package-level encoder would be shared
// mutable state in a package whose whole purpose is that its file handling is
// easy to audit.
func compress(data []byte) ([]byte, error) {
	encoder, err := zstd.NewWriter(nil, zstd.WithEncoderLevel(zstd.SpeedDefault))
	if err != nil {
		return nil, fmt.Errorf("construct zstd encoder: %w", err)
	}

	framed := encoder.EncodeAll(data, nil)

	if err := encoder.Close(); err != nil {
		return nil, fmt.Errorf("close zstd encoder: %w", err)
	}

	return framed, nil
}

// decompress decodes a single zstd frame, refusing anything that expands past
// maxDecompressedSize.
func decompress(framed []byte) ([]byte, error) {
	decoder, err := zstd.NewReader(nil, zstd.WithDecoderMaxMemory(maxDecompressedSize))
	if err != nil {
		return nil, fmt.Errorf("construct zstd decoder: %w", err)
	}
	defer decoder.Close()

	data, err := decoder.DecodeAll(framed, nil)
	if err != nil {
		if errors.Is(err, zstd.ErrDecoderSizeExceeded) {
			return nil, fmt.Errorf("%w of %d bytes", ErrTooLarge, maxDecompressedSize)
		}

		return nil, fmt.Errorf("decode zstd frame: %w", err)
	}

	return data, nil
}
