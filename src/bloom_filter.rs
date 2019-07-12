// This a modified version of the code from https://github.com/jasondavies/bloomfilter.js/tree/master
//
// Copyright (c) 2018, Jason Davies
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// * Redistributions of source code must retain the above copyright notice, this
//   list of conditions and the following disclaimer.
//
// * Redistributions in binary form must reproduce the above copyright notice,
//   this list of conditions and the following disclaimer in the documentation
//   and/or other materials provided with the distribution.
//
// * Neither the name of the copyright holder nor the names of its
//   contributors may be used to endorse or promote products derived from
//   this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
// OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::num::Wrapping;

// http://graphics.stanford.edu/~seander/bithacks.html#CountBitsSetParallel
fn popcnt(value: u32) -> u32 {
    let value = value - ((value >> 1) & 0x55555555);
    let value = (value & 0x33333333) + ((value >> 2) & 0x33333333);
    (Wrapping(value + (value >> 4) & 0xf0f0f0f) * Wrapping(0x1010101)).0 >> 24
}

// Fowler/Noll/Vo hashing.
// Nonstandard variation: this function optionally takes a seed value that is incorporated
// into the offset basis. According to http://www.isthe.com/chongo/tech/comp/fnv/index.html
// "almost any offset_basis will serve so long as it is non-zero".
fn fnv_1a(value: &str, seed: Option<u32>) -> u32 {
    let mut a = 2166136261 ^ (seed.unwrap_or(0));
    value.chars().for_each(|c| {
        let d = (c as u32) & 0xff00;
        if d != 0 {
            a = fnv_multiply(a ^ (d >> 8));
        }
        a = fnv_multiply(a ^ ((c as u32) & 0xff));
    });
    fnv_mix(a)
}

// a * 16777619 mod 2**32
fn fnv_multiply(a: u32) -> u32 {
    let mut a = Wrapping(a);
    a = a + (a << 1) + (a << 4) + (a << 7) + (a << 8) + (a << 24);
    a.0
}

// See https://web.archive.org/web/20131019013225/http://home.comcast.net/~bretm/hash/6.html
fn fnv_mix(a: u32) -> u32 {
    let mut a = Wrapping(a);
    a = a + (a << 13);
    a = a ^ (a >> 7);
    a = a + (a << 3);
    a = a ^ (a >> 17);
    a = a + (a << 5);
    a = a & Wrapping(0xffffffff);
    a.0
}

pub struct BloomFilter {
    m: u32,
    k: u32,
    buckets: Vec<u8>,
}

impl BloomFilter {
    // Creates a new bloom filter.  If *m* is an array-like object, with a length
    // property, then the bloom filter is loaded with data from the array, where
    // each element is a 32-bit integer.  Otherwise, *m* should specify the
    // number of bits.  Note that *m* is rounded up to the nearest multiple of
    // 32.  *k* specifies the number of hashing functions.
    pub fn new() -> Self {
        let m = 8 * 1024;
        let n: usize = m / 8;
        let k: u32 = 8;
        let m: u32 = (n as u32) * 8;
        let buckets = vec![0u8; n];

        Self { m, k, buckets }
    }

    pub fn from_slice(slice: &[u8]) -> Self {
        let mut bf = Self::new();
        bf.buckets.clone_from_slice(slice);
        bf
    }

    // See http://willwhim.wpengine.com/2011/09/03/producing-n-hash-functions-by-hashing-only-once/
    fn locations(&self, value: &str) -> Vec<u16> {
        let mut locations_buffer = vec![0u16; self.k as usize];
        let a = fnv_1a(value, None);
        let b = fnv_1a(value, Some(872958581)); // The seed value is chosen randomly
        let mut x = a % self.m;
        for i in 0..self.k {
            locations_buffer[i as usize] = x as u16;
            x = (x + b) % self.m;
        }
        locations_buffer
    }

    pub fn add(&mut self, value: &str) {
        let l = self.locations(value);
        for i in 0..self.k {
            self.buckets[(l[i as usize] / 8) as usize] |= 1 << l[i as usize] % 8;
        }
    }

    pub fn test(&self, value: &str) -> bool {
        let l = self.locations(value);
        for i in 0..self.k {
            let b = l[i as usize];
            if (self.buckets[(b / 8) as usize] & (1 << b % 8)) == 0 {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    extern crate data_encoding;

    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn popcnt_known_values() {
        assert_eq!(popcnt(0), 0);
        assert_eq!(popcnt(1), 1);
        assert_eq!(popcnt(2), 1);
        assert_eq!(popcnt(4), 1);
        assert_eq!(popcnt(8), 1);
        assert_eq!(popcnt(16), 1);
        assert_eq!(popcnt(0x55555555), 16);
        assert_eq!(popcnt(0x00FF00FF), 16);
        assert_eq!(popcnt(0x0000FFFF), 16);
    }

    #[test]
    fn fnv_1a_known_values() {
        assert_eq!(fnv_1a("lyle", None), 1334908444);
        assert_eq!(fnv_1a("lyle", Some(123)), 1631759920);

        assert_eq!(
            fnv_1a("1/fT/HwFDKYio/n/ZzxTXHpy8U0vzogzJhv+gculEhI", None),
            2344490131
        );
        assert_eq!(
            fnv_1a("5Y6kvOWSehVNYBbkFkWjvcyEV584Hr61oN5z5ZDhKsI", None),
            3296761108
        );
        assert_eq!(
            fnv_1a("b1wsxEazqgDI6ZMXfg3FN/5vOFElRkI340FQ7Mjh13s", None),
            709866909
        );
        assert_eq!(
            fnv_1a("bpcfrG1CJ31wrdEJcOxPmXa41+PjkEbdvUrJ2VyGFWA", None),
            2024372548
        );
        assert_eq!(
            fnv_1a("JmrQQJ/cYfehndWIxwYXrbV4upKEAF0kGoDmn6+BBRg", None),
            1830223429
        );
        assert_eq!(
            fnv_1a("OzLSeTgwfpp7Mq4kDZKGpqgSuR8ZkzQb5F8kSC9q4W8", None),
            32938565
        );
        assert_eq!(
            fnv_1a("R8x1axhFobtSMkUi1nUkkugT1CfPLYMP32PNMNpB148", None),
            869529502
        );
        assert_eq!(
            fnv_1a("OzLSeTgwfpp7Mq4kDZKGpqgSuR8ZkzQb5F8kSC9q4W8", None),
            32938565
        );
        assert_eq!(
            fnv_1a("ZI36t/aBiqlHlNp790hdEzEQYgYIkZZn0osEjsJK5oU", None),
            1559313349
        );

        assert_eq!(
            fnv_1a(
                "1/fT/HwFDKYio/n/ZzxTXHpy8U0vzogzJhv+gculEhI",
                Some(2344490131)
            ),
            2482345007
        );
        assert_eq!(
            fnv_1a(
                "5Y6kvOWSehVNYBbkFkWjvcyEV584Hr61oN5z5ZDhKsI",
                Some(3296761108)
            ),
            3727186510
        );
        assert_eq!(
            fnv_1a(
                "b1wsxEazqgDI6ZMXfg3FN/5vOFElRkI340FQ7Mjh13s",
                Some(709866909)
            ),
            2119017804
        );
        assert_eq!(
            fnv_1a(
                "bpcfrG1CJ31wrdEJcOxPmXa41+PjkEbdvUrJ2VyGFWA",
                Some(2024372548)
            ),
            2524436491
        );
        assert_eq!(
            fnv_1a(
                "JmrQQJ/cYfehndWIxwYXrbV4upKEAF0kGoDmn6+BBRg",
                Some(1830223429)
            ),
            1366223816
        );
        assert_eq!(
            fnv_1a(
                "OzLSeTgwfpp7Mq4kDZKGpqgSuR8ZkzQb5F8kSC9q4W8",
                Some(32938565)
            ),
            3503597509
        );
        assert_eq!(
            fnv_1a(
                "R8x1axhFobtSMkUi1nUkkugT1CfPLYMP32PNMNpB148",
                Some(869529502)
            ),
            1351347067
        );
        assert_eq!(
            fnv_1a(
                "OzLSeTgwfpp7Mq4kDZKGpqgSuR8ZkzQb5F8kSC9q4W8",
                Some(32938565)
            ),
            3503597509
        );
        assert_eq!(
            fnv_1a(
                "ZI36t/aBiqlHlNp790hdEzEQYgYIkZZn0osEjsJK5oU",
                Some(1559313349)
            ),
            2819523972
        );
    }

    #[test]
    fn test_bloomfilter_present() {
        let mut bf = BloomFilter::new();
        bf.add("hello");
        bf.add("Bob");

        assert_eq!(bf.test("hello"), true);
        assert_eq!(bf.test("Bob"), true);
    }

    #[test]
    fn test_bloomfilter_not_present() {
        let bf = BloomFilter::new();

        assert_eq!(bf.test("hello"), false);
        assert_eq!(bf.test("Bob"), false);
    }

    #[test]
    fn test_bloomfilter_100_values() {
        let mut present = vec![];
        let mut not_present = vec![];
        for i in 0..100 {
            present.push(i);
            not_present.push(i + 10000);
        }

        let mut bf = BloomFilter::new();

        present
            .iter()
            .for_each(|value| bf.add(&format!("{}", value)));

        present
            .iter()
            .for_each(|value| assert_eq!(bf.test(&format!("{}", value)), true));
        not_present
            .iter()
            .for_each(|value| assert_eq!(bf.test(&format!("{}", value)), false));
    }

    #[test]
    fn test_bloomfilter_from_slice() {
        use self::data_encoding::BASE64_NOPAD;
        let b64_filter = "AAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAQAAAAAQAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAgAAAAAIQAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAABAAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgAgAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIAIAAAAAIAAAAAAAAAEAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAgAAAAAAAIAAAAAAAAAAAAAgAAAQAAAAAAAAAAAAAAAACAACAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAgAAAAAAAAAACAAAAAAAAEAAAAAAAAQAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAACAAAAAAAAAAAAAgAAAAAAAAABAAAAAAACAAAAAAAAAAAAAAAAAAAAAAAAAAAAIAAIAAIAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAACAAAAAAAAAEAAAAAAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAACAIAAAAAA";
        let filter_slice: Vec<u8> = BASE64_NOPAD.decode(b64_filter.as_bytes()).unwrap();
        let bf = BloomFilter::from_slice(&filter_slice);

        assert_eq!(bf.test("1/fT/HwFDKYio/n/ZzxTXHpy8U0vzogzJhv+gculEhI"), true);
        assert_eq!(bf.test("5Y6kvOWSehVNYBbkFkWjvcyEV584Hr61oN5z5ZDhKsI"), true);
        assert_eq!(bf.test("b1wsxEazqgDI6ZMXfg3FN/5vOFElRkI340FQ7Mjh13s"), true);
        assert_eq!(bf.test("bpcfrG1CJ31wrdEJcOxPmXa41+PjkEbdvUrJ2VyGFWA"), true);
        assert_eq!(bf.test("JmrQQJ/cYfehndWIxwYXrbV4upKEAF0kGoDmn6+BBRg"), true);
        assert_eq!(bf.test("OzLSeTgwfpp7Mq4kDZKGpqgSuR8ZkzQb5F8kSC9q4W8"), true);
        assert_eq!(bf.test("R8x1axhFobtSMkUi1nUkkugT1CfPLYMP32PNMNpB148"), true);
        assert_eq!(bf.test("OzLSeTgwfpp7Mq4kDZKGpqgSuR8ZkzQb5F8kSC9q4W8"), true);
        assert_eq!(bf.test("ZI36t/aBiqlHlNp790hdEzEQYgYIkZZn0osEjsJK5oU"), true);

        // Test some other values are not present
        for i in 0..100 {
            assert_eq!(bf.test(&format!("{}", i)), false);
        }
    }
}
