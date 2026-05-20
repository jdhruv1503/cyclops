pub const CRATE_NAME: &str = "cyclops";

pub fn run() -> i32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_stub_is_ready() {
        assert_eq!(CRATE_NAME, "cyclops");
        assert_eq!(run(), 0);
    }
}
