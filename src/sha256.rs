//! Translated from `src/nvim/sha256.c` and `src/nvim/sha256.h`
//! ("FIPS-180-2 compliant SHA-256 implementation").
//!
//! The original's `P(a,b,c,d,e,f,g,h,x,K)` macro is invoked 64 times with
//! its 8 variable arguments rotated by one position each time (e.g.
//! `P(A,B,C,D,E,F,G,H,...)` then `P(H,A,B,C,D,E,F,G,...)`, etc.) - this is
//! mathematically identical to the standard textbook SHA-256 compression
//! round ("shift all 8 working variables by one register each round"),
//! just spelled differently (C macro parameter rotation vs. explicit
//! variable reassignment, since Rust has no textual macro parameter
//! substitution to mirror the original's exact spelling). The
//! `sha256_self_test`/FIPS-180-2 test vectors from the original are kept
//! and used as this translation's own correctness tests, since they are
//! exact, well-known expected outputs.
//!
//! `ROTR(x, n)` uses `u32::rotate_right`, Rust's native equivalent of the
//! original's `(x >> n) | (x << (32 - n))`.
//!
//! `sha256_bytes` returns an owned `String` rather than a pointer into a
//! shared `static` buffer (the original's `static char hexit[...]`, which
//! is overwritten by the next call and not safe to share across threads) -
//! the hex *value* produced is identical either way.

pub const SHA256_BUFFER_SIZE: usize = 64;
pub const SHA256_SUM_SIZE: usize = 32;

#[derive(Clone)]
pub struct ContextSha256T {
    pub total: [u32; 2],
    pub state: [u32; 8],
    pub buffer: [u8; SHA256_BUFFER_SIZE],
}

impl Default for ContextSha256T {
    fn default() -> Self {
        ContextSha256T {
            total: [0; 2],
            state: [0; 8],
            buffer: [0; SHA256_BUFFER_SIZE],
        }
    }
}

const K: [u32; 64] = [
    0x428A_2F98, 0x7137_4491, 0xB5C0_FBCF, 0xE9B5_DBA5, 0x3956_C25B, 0x59F1_11F1, 0x923F_82A4,
    0xAB1C_5ED5, 0xD807_AA98, 0x1283_5B01, 0x2431_85BE, 0x550C_7DC3, 0x72BE_5D74, 0x80DE_B1FE,
    0x9BDC_06A7, 0xC19B_F174, 0xE49B_69C1, 0xEFBE_4786, 0x0FC1_9DC6, 0x240C_A1CC, 0x2DE9_2C6F,
    0x4A74_84AA, 0x5CB0_A9DC, 0x76F9_88DA, 0x983E_5152, 0xA831_C66D, 0xB003_27C8, 0xBF59_7FC7,
    0xC6E0_0BF3, 0xD5A7_9147, 0x06CA_6351, 0x1429_2967, 0x27B7_0A85, 0x2E1B_2138, 0x4D2C_6DFC,
    0x5338_0D13, 0x650A_7354, 0x766A_0ABB, 0x81C2_C92E, 0x9272_2C85, 0xA2BF_E8A1, 0xA81A_664B,
    0xC24B_8B70, 0xC76C_51A3, 0xD192_E819, 0xD699_0624, 0xF40E_3585, 0x106A_A070, 0x19A4_C116,
    0x1E37_6C08, 0x2748_774C, 0x34B0_BCB5, 0x391C_0CB3, 0x4ED8_AA4A, 0x5B9C_CA4F, 0x682E_6FF3,
    0x748F_82EE, 0x78A5_636F, 0x84C8_7814, 0x8CC7_0208, 0x90BE_FFFA, 0xA450_6CEB, 0xBEF9_A3F7,
    0xC671_78F2,
];

#[inline]
fn bsig0(x: u32) -> u32 {
    x.rotate_right(2) ^ x.rotate_right(13) ^ x.rotate_right(22)
}
#[inline]
fn bsig1(x: u32) -> u32 {
    x.rotate_right(6) ^ x.rotate_right(11) ^ x.rotate_right(25)
}
#[inline]
fn ssig0(x: u32) -> u32 {
    x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3)
}
#[inline]
fn ssig1(x: u32) -> u32 {
    x.rotate_right(17) ^ x.rotate_right(19) ^ (x >> 10)
}
#[inline]
fn maj(x: u32, y: u32, z: u32) -> u32 {
    (x & y) | (z & (x | y))
}
#[inline]
fn ch(x: u32, y: u32, z: u32) -> u32 {
    z ^ (x & (y ^ z))
}

/// `sha256_start`
pub fn sha256_start(ctx: &mut ContextSha256T) {
    ctx.total = [0, 0];
    ctx.state = [
        0x6A09_E667,
        0xBB67_AE85,
        0x3C6E_F372,
        0xA54F_F53A,
        0x510E_527F,
        0x9B05_688C,
        0x1F83_D9AB,
        0x5BE0_CD19,
    ];
}

/// `sha256_process`
fn sha256_process(ctx: &mut ContextSha256T, data: &[u8; SHA256_BUFFER_SIZE]) {
    let mut w = [0u32; 64];
    for (i, chunk) in data.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    for t in 16..64 {
        w[t] = ssig1(w[t - 2])
            .wrapping_add(w[t - 7])
            .wrapping_add(ssig0(w[t - 15]))
            .wrapping_add(w[t - 16]);
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = ctx.state;

    for t in 0..64 {
        let t1 = h
            .wrapping_add(bsig1(e))
            .wrapping_add(ch(e, f, g))
            .wrapping_add(K[t])
            .wrapping_add(w[t]);
        let t2 = bsig0(a).wrapping_add(maj(a, b, c));
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }

    ctx.state[0] = ctx.state[0].wrapping_add(a);
    ctx.state[1] = ctx.state[1].wrapping_add(b);
    ctx.state[2] = ctx.state[2].wrapping_add(c);
    ctx.state[3] = ctx.state[3].wrapping_add(d);
    ctx.state[4] = ctx.state[4].wrapping_add(e);
    ctx.state[5] = ctx.state[5].wrapping_add(f);
    ctx.state[6] = ctx.state[6].wrapping_add(g);
    ctx.state[7] = ctx.state[7].wrapping_add(h);
}

/// `sha256_update`
pub fn sha256_update(ctx: &mut ContextSha256T, mut input: &[u8]) {
    if input.is_empty() {
        return;
    }

    let mut left = (ctx.total[0] & (SHA256_BUFFER_SIZE as u32 - 1)) as usize; // left < buf size

    ctx.total[0] = ctx.total[0].wrapping_add(input.len() as u32);
    if (ctx.total[0] as usize) < input.len() {
        ctx.total[1] += 1;
    }

    let fill = SHA256_BUFFER_SIZE - left;

    if left != 0 && input.len() >= fill {
        ctx.buffer[left..left + fill].copy_from_slice(&input[..fill]);
        let block = ctx.buffer;
        sha256_process(ctx, &block);
        input = &input[fill..];
        left = 0;
    }

    while input.len() >= SHA256_BUFFER_SIZE {
        let block: [u8; SHA256_BUFFER_SIZE] = input[..SHA256_BUFFER_SIZE].try_into().unwrap();
        sha256_process(ctx, &block);
        input = &input[SHA256_BUFFER_SIZE..];
    }

    if !input.is_empty() {
        ctx.buffer[left..left + input.len()].copy_from_slice(input);
    }
}

const SHA256_PADDING: [u8; SHA256_BUFFER_SIZE] = {
    let mut p = [0u8; SHA256_BUFFER_SIZE];
    p[0] = 0x80;
    p
};

/// `sha256_finish`
pub fn sha256_finish(ctx: &mut ContextSha256T) -> [u8; SHA256_SUM_SIZE] {
    let high = (ctx.total[0] >> 29) | (ctx.total[1] << 3);
    let low = ctx.total[0] << 3;

    let mut msglen = [0u8; 8];
    msglen[0..4].copy_from_slice(&high.to_be_bytes());
    msglen[4..8].copy_from_slice(&low.to_be_bytes());

    let last = (ctx.total[0] & 0x3F) as usize;
    let padn = if last < 56 { 56 - last } else { 120 - last };

    sha256_update(ctx, &SHA256_PADDING[..padn]);
    sha256_update(ctx, &msglen);

    let mut digest = [0u8; SHA256_SUM_SIZE];
    for i in 0..8 {
        digest[i * 4..i * 4 + 4].copy_from_slice(&ctx.state[i].to_be_bytes());
    }
    digest
}

/// Gets the hex digest of the buffer (`sha256_bytes`).
///
/// Returns the hex digest of `buf`; if `salt` is `Some`, it is mixed in
/// too, exactly like the original's optional `salt`/`salt_len` parameters.
pub fn sha256_bytes(buf: &[u8], salt: Option<&[u8]>) -> std::string::String {
    sha256_self_test();

    let mut ctx = ContextSha256T::default();
    sha256_start(&mut ctx);
    sha256_update(&mut ctx, buf);
    if let Some(salt) = salt {
        sha256_update(&mut ctx, salt);
    }
    let sum = sha256_finish(&mut ctx);

    let mut hexit = std::string::String::with_capacity(SHA256_SUM_SIZE * 2);
    for byte in sum {
        use std::fmt::Write as _;
        write!(hexit, "{byte:02x}").unwrap();
    }
    hexit
}

// These are the standard FIPS-180-2 test vectors.
const SHA_SELF_TEST_MSG: [&str; 2] = [
    "abc",
    "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
];

const SHA_SELF_TEST_VECTOR: [&str; 3] = [
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
    "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
];

/// Perform a test on the SHA256 algorithm (`sha256_self_test`).
///
/// Returns true if no failures were generated. Memoized like the original
/// via two flags (`sha256_self_tested`/`failures` statics), using
/// [`std::sync::atomic::AtomicBool`] for thread-safety rather than plain
/// (racy) C statics - but critically, matching the *order* of the
/// original's logic exactly: the "already tested" flag is set *before*
/// doing the (recursive - see below) test work, not after.
///
/// This function is indirectly reentrant: it calls [`sha256_bytes`] to
/// compute test digests, and `sha256_bytes` itself calls back into
/// `sha256_self_test` first. The original C handles this safely only
/// because `sha256_self_tested` is set to `true` right away, so the
/// reentrant call sees "already tested" and returns immediately without
/// recursing further. An initial translation used `OnceLock::get_or_init`,
/// which only marks itself initialized *after* its closure returns -
/// reentrant calls during initialization deadlock instead of
/// short-circuiting, which a hanging test caught.
pub fn sha256_self_test() -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static SELF_TESTED: AtomicBool = AtomicBool::new(false);
    static FAILURES: AtomicBool = AtomicBool::new(false);

    // swap() atomically reads the old value and sets it to true in one
    // step, exactly mirroring the original's "check flag, then set it"
    // sequence with no gap a reentrant call could land in.
    if SELF_TESTED.swap(true, Ordering::SeqCst) {
        return !FAILURES.load(Ordering::SeqCst);
    }

    let mut failures = false;
    for i in 0..3 {
        let output = if i < 2 {
            sha256_bytes(SHA_SELF_TEST_MSG[i].as_bytes(), None)
        } else {
            let mut ctx = ContextSha256T::default();
            sha256_start(&mut ctx);
            let buf = [b'a'; 1000];
            for _ in 0..1000 {
                sha256_update(&mut ctx, &buf);
            }
            let sum = sha256_finish(&mut ctx);
            let mut s = std::string::String::with_capacity(SHA256_SUM_SIZE * 2);
            for byte in sum {
                use std::fmt::Write as _;
                write!(s, "{byte:02x}").unwrap();
            }
            s
        };
        if output != SHA_SELF_TEST_VECTOR[i] {
            failures = true;
        }
    }
    FAILURES.store(failures, Ordering::SeqCst);
    !failures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_test_passes() {
        assert!(sha256_self_test());
    }

    #[test]
    fn matches_fips_test_vectors_directly() {
        assert_eq!(
            sha256_bytes(b"abc", None),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_bytes(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
                None
            ),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn salt_changes_the_digest() {
        let plain = sha256_bytes(b"hello", None);
        let salted = sha256_bytes(b"hello", Some(b"pepper"));
        assert_ne!(plain, salted);
    }
}
