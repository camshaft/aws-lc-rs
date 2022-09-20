// Copyright 2015-2022 Brian Smith.
//
// Permission to use, copy, modify, and/or distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHORS DISCLAIM ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY
// SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
// OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
// CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

// Modifications copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use aws_lc_ring_facade::{aead, digest, error, hkdf, hmac, test, test_file};

#[test]
fn hkdf_tests() {
    test::run(test_file!("data/hkdf_tests.txt"), |section, test_case| {
        assert_eq!(section, "");
        let alg = {
            let digest_alg = test_case
                .consume_digest_alg("Hash")
                .ok_or(error::Unspecified)?;
            if digest_alg == &digest::SHA256 {
                hkdf::HKDF_SHA256
            } else {
                // TODO: add test vectors for other algorithms
                panic!("unsupported algorithm: {:?}", digest_alg);
            }
        };
        let secret = test_case.consume_bytes("IKM");
        let salt = test_case.consume_bytes("salt");
        let info = test_case.consume_bytes("info");
        let _ = test_case.consume_bytes("PRK");
        let expected_out = test_case.consume_bytes("OKM");

        let salt = hkdf::Salt::new(alg, &salt);

        // TODO: test multi-part info, especially with empty parts.
        let My(out) = salt
            .extract(&secret)
            .expand(&[&info], My(expected_out.len()))
            .unwrap()
            .into();
        assert_eq!(out, expected_out);

        Ok(())
    });
}

#[test]
fn hkdf_output_len_tests() {
    for &alg in &[hkdf::HKDF_SHA256, hkdf::HKDF_SHA384, hkdf::HKDF_SHA512] {
        const MAX_BLOCKS: usize = 255;

        let salt = hkdf::Salt::new(alg, &[]);
        let prk = salt.extract(&[]); // TODO: enforce minimum length.

        {
            // Test zero length.
            let okm = prk.expand(&[b"info"], My(0)).unwrap();
            let result: My<Vec<u8>> = okm.into();
            assert_eq!(&result.0, &[]);
        }

        let max_out_len = MAX_BLOCKS * alg.hmac_algorithm().digest_algorithm().output_len;

        {
            // Test maximum length output succeeds.
            let okm = prk.expand(&[b"info"], My(max_out_len)).unwrap();
            let result: My<Vec<u8>> = okm.into();
            assert_eq!(result.0.len(), max_out_len);
        }

        {
            // Test too-large output fails.
            assert!(prk.expand(&[b"info"], My(max_out_len + 1)).is_err());
        }

        {
            // Test length mismatch (smaller).
            let okm = prk.expand(&[b"info"], My(2)).unwrap();
            let mut buf = [0u8; 1];
            assert_eq!(okm.fill(&mut buf), Err(error::Unspecified));
        }

        {
            // Test length mismatch (larger).
            let okm = prk.expand(&[b"info"], My(2)).unwrap();
            let mut buf = [0u8; 3];
            assert_eq!(okm.fill(&mut buf), Err(error::Unspecified));
        }

        {
            // Control for above two tests.
            let okm = prk.expand(&[b"info"], My(2)).unwrap();
            let mut buf = [0u8; 2];
            assert_eq!(okm.fill(&mut buf), Ok(()));
        }
    }
}

#[test]
/// Try creating various key types via HKDF.
fn hkdf_key_types() {
    for &alg in &[
        hkdf::HKDF_SHA1_FOR_LEGACY_USE_ONLY,
        hkdf::HKDF_SHA256,
        hkdf::HKDF_SHA384,
        hkdf::HKDF_SHA512,
    ] {
        let salt = hkdf::Salt::new(alg, &[]);
        let prk = salt.extract(&[]);
        let okm = prk.expand(&[b"info"], alg.hmac_algorithm()).unwrap();
        let hmac_key = hmac::Key::from(okm);
        assert_eq!(hmac_key.algorithm(), alg.hmac_algorithm());

        let okm = prk.expand(&[b"info"], alg).unwrap();
        let hkdf_salt_key = hkdf::Salt::from(okm);
        assert_eq!(hkdf_salt_key.algorithm(), alg);

        let okm = prk.expand(&[b"info"], alg).unwrap();
        let _hkdf_prk_key = hkdf::Prk::from(okm);

        for aead_alg in [
            &aead::AES_256_GCM,
            &aead::AES_128_GCM,
            &aead::CHACHA20_POLY1305,
        ] {
            let okm = prk.expand(&[b"info"], aead_alg).unwrap();
            let _aead_prk_key = aead::UnboundKey::from(okm);
        }
    }
}

#[test]
fn hkdf_coverage() {
    // Something would have gone horribly wrong for this to not pass, but we test this so our
    // coverage reports will look better.
    assert_ne!(hkdf::HKDF_SHA256, hkdf::HKDF_SHA384);
    assert_eq!(
        "Algorithm(Algorithm(SHA256))",
        format!("{:?}", hkdf::HKDF_SHA256)
    );

    for &alg in &[
        hkdf::HKDF_SHA1_FOR_LEGACY_USE_ONLY,
        hkdf::HKDF_SHA256,
        hkdf::HKDF_SHA384,
        hkdf::HKDF_SHA512,
    ] {
        // Coverage sanity check.
        assert_eq!(alg.clone(), alg);

        // Only using this API to construct a simple PRK.
        let prk = hkdf::Prk::new_less_safe(alg, &[0; 32]);
        let result: My<Vec<u8>> = prk
            .expand(&[b"info"], My(digest::MAX_OUTPUT_LEN))
            .unwrap()
            .into();

        let prk_clone = prk.clone();
        let result_2: My<Vec<u8>> = prk_clone
            .expand(&[b"info"], My(digest::MAX_OUTPUT_LEN))
            .unwrap()
            .into();
        assert_eq!(result, result_2);
    }
}

/// Generic newtype wrapper that lets us implement traits for externally-defined
/// types.
#[derive(Debug, PartialEq)]
struct My<T: core::fmt::Debug + PartialEq>(T);

impl hkdf::KeyType for My<usize> {
    fn len(&self) -> usize {
        self.0
    }
}

impl From<hkdf::Okm<'_, My<usize>>> for My<Vec<u8>> {
    fn from(okm: hkdf::Okm<My<usize>>) -> Self {
        let mut r = vec![0u8; okm.len().0];
        okm.fill(&mut r).unwrap();
        Self(r)
    }
}
