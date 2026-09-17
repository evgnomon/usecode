//! Wyhash (final version), byte-for-byte compatible with Zig's `std.hash.Wyhash`.
//!
//! vm derives a VM's MAC address and cloud-init instance-id from this hash, so the
//! output must stay identical to the Zig implementation for VMs created before the
//! Rust port to keep resolving to the same MAC.

const SECRET: [u64; 4] = [
    0xa076_1d64_78bd_642f,
    0xe703_7ed1_a0b4_28db,
    0x8ebc_6af0_9c88_c6e3,
    0x5899_65cc_7537_4cc3,
];

/// Hashes `input` with `seed`.
pub fn hash(seed: u64, input: &[u8]) -> u64 {
    let mut state = Wyhash::new(seed);

    if input.len() <= 16 {
        state.small_key(input);
    } else {
        let mut i = 0;
        if input.len() >= 48 {
            while i + 48 < input.len() {
                state.round(&input[i..i + 48]);
                i += 48;
            }
            state.final0();
        }
        state.final1(input, i);
    }

    state.total_len = input.len();
    state.final2()
}

struct Wyhash {
    a: u64,
    b: u64,
    state: [u64; 3],
    total_len: usize,
}

impl Wyhash {
    fn new(seed: u64) -> Self {
        let s = seed ^ mix(seed ^ SECRET[0], SECRET[1]);
        Wyhash {
            a: 0,
            b: 0,
            state: [s; 3],
            total_len: 0,
        }
    }

    fn small_key(&mut self, input: &[u8]) {
        debug_assert!(input.len() <= 16);

        if input.len() >= 4 {
            let end = input.len() - 4;
            let quarter = (input.len() >> 3) << 2;
            self.a = (read(4, &input[0..]) << 32) | read(4, &input[quarter..]);
            self.b = (read(4, &input[end..]) << 32) | read(4, &input[end - quarter..]);
        } else if !input.is_empty() {
            self.a = ((input[0] as u64) << 16)
                | ((input[input.len() >> 1] as u64) << 8)
                | (input[input.len() - 1] as u64);
            self.b = 0;
        } else {
            self.a = 0;
            self.b = 0;
        }
    }

    fn round(&mut self, input: &[u8]) {
        debug_assert!(input.len() >= 48);
        for i in 0..3 {
            let a = read(8, &input[16 * i..]);
            let b = read(8, &input[16 * i + 8..]);
            self.state[i] = mix(a ^ SECRET[i + 1], b ^ self.state[i]);
        }
    }

    fn final0(&mut self) {
        self.state[0] ^= self.state[1] ^ self.state[2];
    }

    /// `input_lb` must be at least 16 bytes long; shorter keys go through `small_key`.
    fn final1(&mut self, input_lb: &[u8], start_pos: usize) {
        debug_assert!(input_lb.len() >= 16);
        debug_assert!(input_lb.len() - start_pos <= 48);
        let input = &input_lb[start_pos..];

        let mut i = 0;
        while i + 16 < input.len() {
            self.state[0] = mix(
                read(8, &input[i..]) ^ SECRET[1],
                read(8, &input[i + 8..]) ^ self.state[0],
            );
            i += 16;
        }

        self.a = read(8, &input_lb[input_lb.len() - 16..]);
        self.b = read(8, &input_lb[input_lb.len() - 8..]);
    }

    fn final2(&mut self) -> u64 {
        self.a ^= SECRET[1];
        self.b ^= self.state[0];
        mum(&mut self.a, &mut self.b);
        mix(
            self.a ^ SECRET[0] ^ (self.total_len as u64),
            self.b ^ SECRET[1],
        )
    }
}

fn read(bytes: usize, data: &[u8]) -> u64 {
    debug_assert!(bytes <= 8);
    let mut buf = [0u8; 8];
    buf[..bytes].copy_from_slice(&data[..bytes]);
    u64::from_le_bytes(buf)
}

fn mum(a: &mut u64, b: &mut u64) {
    let x = (*a as u128).wrapping_mul(*b as u128);
    *a = x as u64;
    *b = (x >> 64) as u64;
}

fn mix(a: u64, b: u64) -> u64 {
    let mut a = a;
    let mut b = b;
    mum(&mut a, &mut b);
    a ^ b
}

#[cfg(test)]
mod tests {
    use super::hash;

    // https://github.com/wangyi-fudan/wyhash test vectors, as used by Zig's std.
    #[test]
    fn test_vectors() {
        let vectors: &[(u64, u64, &str)] = &[
            (0, 0x0409_638e_e2bd_e459, ""),
            (1, 0xa841_2d09_1b5f_e0a9, "a"),
            (2, 0x32dd_92e4_b291_5153, "abc"),
            (3, 0x8619_1240_89a3_a16b, "message digest"),
            (4, 0x7a43_afb6_1d7f_5f40, "abcdefghijklmnopqrstuvwxyz"),
            (
                5,
                0xff42_329b_90e5_0d58,
                "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
            ),
            (
                6,
                0xc39c_ab13_b115_aad3,
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
            ),
        ];

        for (seed, expected, input) in vectors {
            assert_eq!(hash(*seed, input.as_bytes()), *expected, "input: {input}");
        }
    }
}
