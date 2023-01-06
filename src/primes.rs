/*!
A primality checker, suitable for programs that need to repeatedly check
integers for primality.

This is not thread-safe, and if used by multiple threads should be
wrapped in a Mutex.
*/

// Return an upper bound on the square root of `n`.
fn sqrt_sup(n: u64) -> u64 {
    let x = n as f64;
    x.sqrt().ceil() as u64
}

pub struct Primes {
    known: Vec<u64>
}

impl Default for Primes {
    fn default() -> Self {
        Self { known: vec![2] }
    }
}

impl Primes {
    /**
    Create a prime checker with pre-allocated internal capacity for `cap`
    primes.
    */ 
    pub fn with_capacity(cap: usize) -> Self {
        let mut known: Vec<u64> = Vec::with_capacity(cap);
        known.push(2);

        Self { known }
    }

    // The element of `self.known` *must* exceed `sqrt_n_sup` before calling.
    // `sqrt_n_sup` must be equal to or greater than `sqrt(n). 
    fn check(&self, n: u64, sqrt_n_sup: u64) -> bool {
        for &p in self.known.iter() {
            if p > sqrt_n_sup {
                return true;
            } else if n == p {
                return true;
            }else if n % p == 0 {
                return false;
            }
        }
        // We shouldn't ever get here, but just in case.
        return true
    }

    fn append_next(&mut self) {
        // This unwrapping should be fine because all public constructors
        // ensure `self.known` has at least one element.
        let mut next = *self.known.last().unwrap() + 1;

        loop {
            let next_sqrt_sup = sqrt_sup(next);
            if self.check(next, next_sqrt_sup) {
                self.known.push(next);
                return;
            }
            next += 1;
        }
    }

    pub fn is_prime(&mut self, n: u64) -> bool {
        if n < 2 { return false; }

        let sqrt_n_sup = sqrt_sup(n);

        while *self.known.last().unwrap() < sqrt_n_sup {
            self.append_next();
        }

        self.check(n, sqrt_n_sup)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_primes() {
        let mut pz = Primes::default();

        for n in 0..100 {
            println!("{}: {}", n, pz.is_prime(n));
        }
    }
}